"""Separate pre-dispatch cancellation from discarding a real worker proposal."""
from playwright.sync_api import expect


def verify_cancellation(page, ready, result):
    proof = result['cancellation'] = {}

    def controls():
        return page.evaluate('''() => Object.fromEntries(['s-undo','s-redo','s-export'].map(id=>
            [id,{disabled:document.getElementById(id).disabled}]))''')

    def before():
        page.evaluate('() => regionUi.remember()')
        return {'audio': page.evaluate('() => regionUi.fingerprint()'), 'controls': controls()}

    def unchanged(saved):
        page.evaluate('() => regionUi.unchanged(true)')
        current = {'audio': page.evaluate('() => regionUi.fingerprint()'), 'controls': controls()}
        assert current == saved, 'Cancellation changed preview, bank, metadata or history controls'
        return current

    saved = before()
    start = page.evaluate('regionUi.calls.length')
    # The migration no-op is still asynchronous. Cancelling in this task must
    # stop dispatch, not allocate a proposal just to manufacture a discard.
    page.evaluate("() => { document.querySelector('#s-edit-reverse').click(); document.querySelector('#s-cancel').click(); }")
    ready(page)
    expect(page.locator('#s-status')).to_contain_text('Cancelled')
    after = unchanged(saved)
    calls = page.evaluate('start => regionUi.calls.slice(start)', start)
    assert not any(call['action'] in ('region', 'commit', 'discard') for call in calls), calls
    proof['before_dispatch'] = {'passed': True, 'before': saved, 'after': after, 'calls': calls}

    saved = before()
    start = page.evaluate('regionUi.calls.length')
    replies = page.evaluate('regionUi.replies.length')
    page.evaluate("regionUi.holdNextReply('region')")
    page.locator('#s-edit-reverse').click()
    page.wait_for_function('() => regionUi.heldReply?.worker_replied === true', timeout=15000)
    held = page.evaluate('regionUi.heldReply')
    assert held['requested'] and isinstance(held['proposal'], int) and not held['released'], held
    # The real worker has computed its fork, while the app still owns the old
    # committed revision. No fake DSP result or cancellation handler is used.
    page.evaluate('() => regionUi.unchanged(true)')
    expect(page.locator('#s-export')).to_be_disabled()
    try:
        page.locator('#s-cancel').click()
        expect(page.locator('#s-status')).to_contain_text('Cancelling')
        assert not page.evaluate('regionUi.heldReply.released')
    finally:
        page.evaluate('regionUi.releaseHeldReply()')
    ready(page)
    expect(page.locator('#s-status')).to_contain_text('Cancelled')
    after = unchanged(saved)
    calls = page.evaluate('start => regionUi.calls.slice(start)', start)
    delivered = page.evaluate('start => regionUi.replies.slice(start)', replies)
    assert len([call for call in calls if call['action'] == 'region']) == 1, calls
    discarded = [call for call in calls if call['action'] == 'discard']
    assert len(discarded) == 1 and discarded[0]['proposal'] == held['proposal'], calls
    assert not any(call['action'] == 'commit' for call in calls), calls
    assert any(reply['action'] == 'discard' and reply['proposal'] == held['proposal'] for reply in delivered), delivered
    assert page.evaluate('regionUi.heldReply.delivered'), 'Held result never reached the actual cancellation guard'
    proof['in_flight'] = {'passed': True, 'held': held, 'before': saved, 'after': after,
                          'calls': calls, 'replies': delivered}
    return ['same-task Cancel before dispatch submits no region/commit/discard and preserves exact preview/bank/history',
            'Cancel after actual held worker proposal discards that exact proposal without commit; preview/bank/history remain exact']
