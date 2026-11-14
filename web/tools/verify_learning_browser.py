"""Exercise learning persistence, disclosure, keyboard focus and phone layouts."""
import argparse
import functools
import http.server
import json
from pathlib import Path
import threading
import time
from urllib.parse import parse_qs, urlparse

from playwright.sync_api import sync_playwright, expect
from browser_capture import BrowserDiagnostics
from verify_studio_browser import fixture

ROOT = Path(__file__).resolve().parents[2]
KEY = 'slimeybeats.interface-preferences'
TERMINAL_KEY = KEY + '.tutorial-dismissed'


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *unused):
        pass


def choices(page):
    return page.evaluate('(key) => JSON.parse(localStorage.getItem(key))', KEY)


def choose_page(page, key, touch=False):
    page.evaluate('() => new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))')
    if not page.locator('#page-navigation').evaluate('el=>el.open'):
        page.locator('#page-navigation-summary').click()
    getattr(page.locator('#tab-'+key), 'tap' if touch else 'click')()


def choose_controls(page, mode):
    page.evaluate('() => new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))')
    if not page.locator('#control-detail-panel').evaluate('el=>el.open'):
        page.locator('#control-detail-summary').click()
    page.locator('#controls-'+mode).click()


def audio_ui(page):
    return page.evaluate('''() => ({jobs:window.learningJobs, fields:[...document.querySelectorAll(
        '.direction input,.direction select,#bands input,#bands select,#eq-output,#hold,#match,#volume,#s-format'
    )].map(el=>[el.id || el.getAttribute('aria-label'),el.value,el.checked]),
        source:document.querySelector('#original').className,
        master:document.querySelector('#master-source').className,
        filename:document.querySelector('#filename').textContent,
        play:document.querySelector('#play').textContent,
        export:document.querySelector('#export').disabled})''')


def no_overflow(page):
    size = page.evaluate('''() => ({scroll:document.documentElement.scrollWidth,
        client:document.documentElement.clientWidth, visual:visualViewport.width, inner:innerWidth})''')
    assert size['scroll'] <= size['client'] + 1 and size['client'] <= size['visual'] + 1, (size, page.evaluate('''() =>
        [...document.querySelectorAll('body *')].filter(el=>{const r=el.getBoundingClientRect();
        return r.width && (r.right>innerWidth+1 || r.left < -1 || el.scrollWidth>el.clientWidth+1) && getComputedStyle(el).position!=='absolute';})
        .map(el=>[el.tagName,el.id,el.className,el.scrollWidth,el.clientWidth]).slice(0,12)'''))


def hold_worker_result(page, module, class_name, action):
    # The real worker finishes normally. Only delivering its result to the UI is
    # held, giving pointer hit tests a deterministic pending-job window.
    page.evaluate('''async ({module, className, action}) => {
        delete window.releaseLearningJob;
        const resource=performance.getEntriesByType('resource').find(e=>e.name.includes('/'+module)).name;
        const Client=(await import(resource))[className], call=Client.prototype.call;
        Client.prototype.call=function(next,...args) {
            const result=call.call(this,next,...args);
            if(next!==action)return result;
            Client.prototype.call=call;
            return result.then(value=>new Promise(resolve=>{window.releaseLearningJob=()=>resolve(value)}));
        };
    }''', {'module': module, 'className': class_name, 'action': action})


def unobscured(page, selector, ceiling):
    box = page.locator(selector).bounding_box()
    assert box and box['y'] >= 0 and box['y'] + box['height'] <= ceiling + 1, (selector, box, ceiling)


def finish_tour(page):
    while page.locator('#tutorial-next').count():
        page.locator('#tutorial-next').click()
    page.locator('#tutorial-finish').click()
    expect(page.locator('#learning-dialog')).not_to_be_visible()
    assert choices(page)['tutorial'] == 'completed'


def observe_page(page, args, result, profile='fresh'):
    capture = BrowserDiagnostics(page,args.browser,result,interface_profile=profile)
    page.on('request',lambda request:result['requests'].append({'method':request.method,'url':request.url,'body_bytes':len(request.post_data_buffer or b'')}))
    page.on('requestfailed',lambda request:result.setdefault('network_failures',[]).append(
        {'url':request.url,'failure':request.failure}))
    return capture


def whole_words(page, selectors):
    broken = page.evaluate('''selectors => {
        const broken=[];
        for(const host of document.querySelectorAll(selectors)) {
            if(!host.getClientRects().length)continue;
            const walk=document.createTreeWalker(host,NodeFilter.SHOW_TEXT);
            for(let node;node=walk.nextNode();) {
                if(!node.parentElement.getClientRects().length)continue;
                for(const match of node.textContent.matchAll(/[A-Za-z]+(?:['’][A-Za-z]+)*/g)) {
                    const range=document.createRange();range.setStart(node,match.index);range.setEnd(node,match.index+match[0].length);
                    const lines=new Set([...range.getClientRects()].filter(r=>r.width&&r.height).map(r=>Math.round(r.top)));
                    if(lines.size>1)broken.push({host:host.id||host.className,word:match[0],lines:[...lines]});
                }
            }
        }
        return broken;
    }''', selectors)
    assert not broken, broken


def first_answer_line(page, selector):
    geometry=page.locator(selector).first.evaluate(r'''el=>{
        const content=el.closest('.learning-content').getBoundingClientRect();
        const walk=document.createTreeWalker(el,NodeFilter.SHOW_TEXT);
        let text;while((text=walk.nextNode())&&!text.textContent.trim()){}
        const match=/\S+/.exec(text.textContent),range=document.createRange();
        range.setStart(text,match.index);range.setEnd(text,match.index+match[0].length);
        const line=range.getBoundingClientRect();
        return {top:line.top,bottom:line.bottom,paneTop:content.top,paneBottom:content.bottom};
    }''')
    assert geometry['top']>=geometry['paneTop'] and geometry['bottom']<=geometry['paneBottom'],geometry
    return geometry


def verify_help_reachability(browser, base, args, result):
    result['help_answers']=[]
    for width,height in [(320,568),(390,844),(1100,720)]:
        for percent in [100,200]:
            context=browser.new_context(viewport={'width':width,'height':height},has_touch=width<680,
                **({'is_mobile':width<680} if args.browser!='firefox' else {}))
            page=context.new_page()
            capture=observe_page(page,args,result,profile='processing')
            activate='tap' if width<680 else 'click'
            tag=f'help-{width}x{height}-{percent}'
            try:
                page.goto(base,wait_until='networkidle')
                expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL',timeout=30000)
                page.evaluate('(percent)=>document.documentElement.style.fontSize=percent+"%"',percent)
                before=audio_ui(page)
                getattr(page.locator('#open-learning'),activate)()
                getattr(page.locator('#learning-topics'),activate)()
                expect(page.locator('#help-quickstart')).to_be_focused()
                getattr(page.locator('#help-mcp'),activate)()
                expect(page.locator('#learning-title')).to_have_text('AI / MCP setup')
                expect(page.locator('#learning-title')).to_be_focused()
                unobscured(page,'#learning-title',page.locator('#learning-actions').bounding_box()['y'])
                answer=first_answer_line(page,'.learning-callout')
                for key in ['learning-close','learning-topics','learning-done']:
                    unobscured(page,'#'+key,height)
                capture.capture(args.out/(tag+'-mcp-answer.png'))
                focus_path=[]
                for _ in range(16):
                    page.keyboard.press('Tab')
                    focus_path.append(page.evaluate('document.activeElement.id || document.activeElement.className'))
                    assert page.evaluate("document.querySelector('#learning-dialog').contains(document.activeElement)")
                    if page.locator('#learning-topics').evaluate('e=>e===document.activeElement'):break
                expect(page.locator('#learning-topics')).to_be_focused()
                page.keyboard.press('Enter')
                expect(page.locator('#help-quickstart')).to_be_focused()
                page.keyboard.press('Tab');page.keyboard.press('Enter')
                expect(page.locator('#glossary-search')).to_be_focused()
                page.keyboard.type('LUFS')
                expect(page.locator('#glossary-search')).to_be_focused()
                expect(page.locator('.learning-glossary dt')).to_have_text('LUFS')
                definition=first_answer_line(page,'.learning-glossary dd')
                unobscured(page,'#glossary-search',page.locator('#learning-actions').bounding_box()['y'])
                capture.capture(args.out/(tag+'-glossary-answer.png'))
                page.keyboard.press('Tab');page.keyboard.press('Enter')
                expect(page.locator('#help-quickstart')).to_be_focused()
                page.keyboard.press('Tab');page.keyboard.press('Tab');page.keyboard.press('Enter')
                expect(page.locator('#learning-title')).to_have_text('Fix a problem')
                expect(page.locator('#learning-title')).to_be_focused()
                first_answer_line(page,'#learning-body summary')
                getattr(page.locator('#learning-done'),activate)()
                expect(page.locator('#open-learning')).to_be_focused()
                getattr(page.locator('#open-learning'),activate)()
                page.keyboard.press('Escape')
                expect(page.locator('#open-learning')).to_be_focused()
                assert audio_ui(page)==before
                no_overflow(page)
                result['help_answers'].append({'width':width,'height':height,'text_percent':percent,
                    'mcp_first_line':answer,'glossary_definition_first_line':definition,
                    'typing_focus_preserved':True,'close_and_topics_available':True,'keyboard_topics_path':focus_path})
            except Exception:
                capture.capture(args.out/(tag+'-failure.png'))
                raise
            finally:context.close()
    result['checks'].append('W05: 320/390/1100 at 100/200% topic answers appear with focused headings; pinned Topics and Close work by touch/keyboard, Glossary typing reveals the definition without stealing focus, Close/Escape return to Help and preserve audio fields')


def verify_interleaved_terminal(browser, base, args, result):
    result['interleaved_terminal'] = []
    for terminal in ['completed', 'declined']:
        context = browser.new_context()
        held, released = [], False
        try:
            page = context.new_page()
            observe_page(page, args, result)
            page.route('**/assets/learning-guide.json', lambda route: held.append(route))
            page.goto(base, wait_until='domcontentloaded')
            expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
            page.evaluate('''({key,marker,terminal})=>{
                const write=Storage.prototype.setItem;
                window.interleavingArmed=true;
                Storage.prototype.setItem=function(name,value) {
                    if(name===key && window.interleavingArmed) {
                        window.interleavingArmed=false;
                        // A terminal save occurs after all of this writer's
                        // reads, immediately before its stale profile write.
                        write.call(this,marker,terminal);
                        write.call(this,key,JSON.stringify({version:1,advanced:false,tutorial:terminal,step:7}));
                    }
                    return write.call(this,name,value);
                };
            }''', {'key': KEY, 'marker': TERMINAL_KEY, 'terminal': terminal})
            choose_controls(page, 'advanced')
            assert page.evaluate('window.interleavingArmed') is False
            stale = choices(page)
            assert stale['tutorial'] == 'unseen', stale
            assert page.evaluate('key=>localStorage.getItem(key)', TERMINAL_KEY) == terminal
            newest = context.new_page()
            observe_page(newest, args, result)
            newest.goto(base, wait_until='networkidle')
            expect(newest.locator('#learning-dialog')).not_to_be_visible()
            newest.reload(wait_until='networkidle')
            expect(newest.locator('#learning-dialog')).not_to_be_visible()
            newest.locator('#open-learning').click()
            newest.locator('#replay-tutorial').click()
            newest.keyboard.press('Escape')
            assert newest.evaluate('key=>localStorage.getItem(key)', TERMINAL_KEY) == terminal
            released = True
            held[0].continue_()
            expect(page.locator('#learning-dialog')).not_to_be_visible()
            result['interleaved_terminal'].append({'terminal':terminal,'stale_profile_after_race':stale,'marker':terminal,'fresh_tab_and_reload_suppressed':True})
        finally:
            if held and not released:
                held[0].continue_()
                page.wait_for_load_state('networkidle')
            context.close()
    result['checks'].append('deterministic terminal write between profile reads/write: stale profile can say unseen, but the terminal-only marker suppresses fresh-tab/reload offers and survives explicit replay')


def verify_review_layouts(browser, base, args, result, audio_supported):
    result['review_layouts'] = []
    for width,height in [(320,568),(390,844),(430,700),(1100,720)]:
        for percent in [100,200]:
            context = browser.new_context(viewport={'width':width,'height':height},has_touch=width<680,
                **({'is_mobile':width<680} if args.browser!='firefox' else {}))
            page = context.new_page()
            capture = observe_page(page,args,result)
            page.on('dialog',lambda dialog:dialog.accept())
            try:
                page.goto(base,wait_until='networkidle')
                if percent==200:page.evaluate("document.documentElement.style.fontSize='200%'")
                page.evaluate('() => new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))')
                tag=f'review-{width}x{height}-{percent}'
                for choice in ['#tutorial-decline','#tutorial-start']:
                    unobscured(page,choice,height)
                whole_words(page,'.learning-heading,#learning-title,#learning-description,#learning-body p,#learning-actions button')
                capture.capture(args.out/(tag+'-welcome.png'))
                page.locator('#tutorial-decline').click()
                choose_page(page,'studio-tools')
                page.evaluate('scrollTo(0,0)')
                unobscured(page,'#s-source',page.locator('.help-dock').bounding_box()['y'])
                assert page.locator('#open-learning').bounding_box()['x'] > width/2
                no_overflow(page)
                capture.capture(args.out/(tag+'-empty-studio.png'))
                page.locator('#s-target').scroll_into_view_if_needed()
                page.locator('#s-target').focus()
                source_choice=page.locator('.edit-target').evaluate('''el=>({
                    client:el.clientWidth,scroll:el.scrollWidth,
                    selectWidth:el.querySelector('select').getBoundingClientRect().width})''')
                assert source_choice['scroll']<=source_choice['client']+1,source_choice
                no_overflow(page)
                whole_words(page,'.edit-target')
                unobscured(page,'#s-target',page.locator('.help-dock').bounding_box()['y'])
                page.locator('#s-target').select_option('preview')
                expect(page.locator('#s-target')).to_have_value('preview')
                page.locator('#s-target').select_option('source')
                expect(page.locator('#s-target')).to_have_value('source')
                capture.capture(args.out/(tag+'-source-choice.png'))
                page.locator('.skip-link').focus();page.keyboard.press('Enter')
                skip_target='#page-navigation-summary' if page.locator('body').evaluate("e=>e.classList.contains('compact-workspace')") else '#workspace-controls'
                expect(page.locator(skip_target)).to_be_focused()
                if not page.locator('#control-detail-panel').evaluate('e=>e.open'):
                    page.locator('#control-detail-summary').click()
                page.locator('#controls-advanced').scroll_into_view_if_needed()
                whole_words(page,'#controls-basic,#controls-advanced,#control-detail-summary,.brand strong')
                capture.capture(args.out/(tag+'-visibility-labels.png'))
                choose_controls(page,'basic')
                if not page.locator('#page-navigation').evaluate('e=>e.open'):
                    page.locator('#page-navigation-summary').click()
                page.locator('#tab-report').scroll_into_view_if_needed()
                whole_words(page,'.tabs button,#page-navigation-summary')
                capture.capture(args.out/(tag+'-navigation-labels.png'))
                page.locator('#tab-report').click()
                expect(page.locator('#report')).to_be_visible()
                choose_page(page,'overview')
                metrics={'width':width,'height':height,'text_percent':percent,'source_initially_visible':True,'whole_word_labels':True,'source_choice':source_choice}
                if audio_supported:
                    page.locator('#file').set_input_files(fixture('review-layout.wav',seconds=2))
                    expect(page.locator('#master')).to_be_enabled(timeout=30000)
                    page.locator('#mobile-master' if page.locator('#mobile-master').is_visible() else '#master').click()
                    expect(page.locator('#export')).to_be_enabled(timeout=120000)
                    for studio in [False,True]:
                        if studio:
                            choose_page(page,'studio-tools')
                            page.locator('#s-use-main').click()
                            expect(page.locator('#s-export')).to_be_enabled(timeout=30000)
                        player='.edit-player' if studio else '.playback'
                        ids=['s-play','s-stop','s-before','s-after'] if studio else ['play','stop','original','master-source']
                        box=page.locator(player).bounding_box()
                        for key in ids:unobscured(page,'#'+key,page.locator('.help-dock').bounding_box()['y'])
                        sizes=page.locator(player).evaluate('e=>({client:e.clientHeight,scroll:e.scrollHeight,width:e.clientWidth,scrollWidth:e.scrollWidth})')
                        assert sizes['scroll']<=sizes['client']+1 and sizes['scrollWidth']<=sizes['width']+1,sizes
                        assert box['height'] < height*.25,(box,height)
                        no_overflow(page)
                        whole_words(page,player+' button,'+player+' .transport-context')
                        capture.capture(args.out/(tag+('-studio-player.png' if studio else '-master-player.png')))
                        metrics['studio_player' if studio else 'master_player']={'box':box,'content':sizes}
                        page.locator('#'+ids[0]).click()
                        expect(page.locator('#'+ids[0])).to_have_text('Pause',timeout=30000)
                        page.locator('#'+ids[2]).click()
                        page.locator('#'+ids[3]).click()
                        page.locator('#'+ids[1]).click()
                        page.locator('#playback-details-toggle').click()
                        detail='#s-playback-details' if studio else '#playback-details'
                        expect(page.locator(detail)).to_have_attribute('open','')
                        assert page.locator(detail).evaluate("e=>getComputedStyle(e).position") not in ['fixed','sticky']
                        page.locator('#playback-details-toggle').click()
                        if width==320:
                            if studio:page.locator('#s-tab-voice').click()
                            hold_worker_result(page,'studio-client.js' if studio else 'dsp-client.js','StudioClient' if studio else 'DspClient','voice' if studio else 'master')
                            if studio:page.locator('#s-voice').click()
                            else:page.locator('#mobile-master').click()
                            page.wait_for_function('()=>typeof window.releaseLearningJob==="function"')
                            cancel='#job-cancel-studio' if studio else '#job-cancel-master'
                            unobscured(page,cancel,page.locator(player).bounding_box()['y'])
                            capture.capture(args.out/(tag+('-studio-busy.png' if studio else '-master-busy.png')))
                            page.locator(cancel).click()
                            if not studio:expect(page.locator('#status')).to_contain_text('Cancelled. Your original is ready',timeout=30000)
                            page.evaluate('window.releaseLearningJob()')
                            if studio:expect(page.locator('#s-status')).to_contain_text('Cancelled',timeout=30000)
                            if studio:
                                paragraph=page.locator('#s-voice-page > p').first
                                paragraph.scroll_into_view_if_needed()
                                whole_words(page,'#s-voice-page > p,#studio-tools > p.muted,#s-status')
                                capture.capture(args.out/(tag+'-paragraphs.png'))
                result['review_layouts'].append(metrics)
            finally:context.close()
    result['checks'].append('W01/W04: real 100/200% words remain whole; 320×568 welcome choices and empty Studio import start visible' + ('; W02 loaded primary Play/Stop/A/B fit without nested scrolling and details expand in page flow' if audio_supported else '; loaded-audio layout cases unavailable in this runtime'))


def verify_terminal_storage_failures(browser, base, args, result):
    for failed_key in [KEY, TERMINAL_KEY]:
        context=browser.new_context()
        context.add_init_script('''(() => {
            const failed='''+json.dumps(failed_key)+''',write=Storage.prototype.setItem;
            window.choiceSaveFailures=0;
            Storage.prototype.setItem=function(key,value) {
                if(key===failed){window.choiceSaveFailures++;throw new DOMException('blocked choice key','QuotaExceededError');}
                return write.call(this,key,value);
            };
        })();''')
        try:
            page=context.new_page();observe_page(page,args,result)
            page.goto(base,wait_until='networkidle')
            page.locator('#tutorial-decline').click()
            expect(page.locator('#ui-storage-notice')).to_contain_text('cannot remember')
            before=page.evaluate('window.choiceSaveFailures')
            choose_controls(page,'advanced')
            assert page.evaluate('window.choiceSaveFailures')==before
            newest=context.new_page();observe_page(newest,args,result)
            newest.goto(base,wait_until='networkidle')
            expect(newest.locator('#learning-dialog')).not_to_be_visible()
            assert newest.evaluate('window.choiceSaveFailures')<=1
        finally:context.close()
    context=browser.new_context()
    context.add_init_script('localStorage.setItem('+json.dumps(TERMINAL_KEY)+',"unseen")')
    try:
        page=context.new_page();observe_page(page,args,result)
        page.goto(base,wait_until='networkidle')
        expect(page.locator('#ui-storage-notice')).to_contain_text('could not be read')
        expect(page.locator('#learning-dialog')).not_to_be_visible()
        page.locator('#open-learning').click();page.locator('#replay-tutorial').click();page.keyboard.press('Escape')
        assert page.evaluate('key=>localStorage.getItem(key)',TERMINAL_KEY)=='unseen'
    finally:context.close()
    result['checks'].append('terminal-marker/profile partial write failures preserve the other terminal save, warn without retry loops, and suppress future offers; corrupt marker suppresses automatic guidance and remains untouched')


def verify_pending_feedback(browser, base, args, result):
    context=browser.new_context(viewport={'width':320,'height':568},has_touch=True,
        **({'is_mobile':True} if args.browser!='firefox' else {}))
    page=context.new_page()
    capture=observe_page(page,args,result,profile='processing')
    page.on('dialog',lambda dialog:dialog.accept())
    try:
        page.goto(base,wait_until='networkidle')
        choose_page(page,'studio-tools')
        page.locator('.edit-import summary').click()
        page.locator('#s-mode').select_option('four')
        for kind,hz in [('vocals',440),('drums',220),('bass',110),('other',660)]:
            page.locator(f'#s-stem-files input[data-kind="{kind}"]').set_input_files(fixture(kind+'.wav',hz,seconds=8))
        page.locator('#s-import').click()
        expect(page.locator('#s-export')).to_be_enabled(timeout=30000)
        gain=page.locator('#s-faders input[data-control="gain_db"]').first
        for turn in [1,2]:
            page.locator('#s-before').click()
            gain.scroll_into_view_if_needed();gain.focus();page.keyboard.press('ArrowLeft')
            expect(page.locator('#s-status')).to_contain_text('Stem controls are live on B')
            expect(page.locator('#s-status')).to_contain_text('Update mix preview')
            expect(page.locator('#s-status')).not_to_contain_text('Download prepared')
            expect(page.locator('#s-compare-state')).to_contain_text('B Live stems')
            assert 'selected' in page.locator('#s-after').get_attribute('class').split()
            expect(page.locator('#s-export')).to_be_disabled()
            expect(page.locator('#s-quick-mix')).to_be_enabled()
            unobscured(page,'#s-quick-mix',page.locator('.help-dock').bounding_box()['y'])
            capture.capture(args.out/f'pending-mix-{turn}-active-edit.png')
            page.locator('#s-play').click();expect(page.locator('#s-play')).to_have_text('Pause')
            page.locator('#s-after').click();page.locator('#s-stop').click()
            choose_page(page,'studio-tools')
            page.locator('#s-tab-voice').click()
            expect(page.locator('#s-status')).to_contain_text('Stem controls are live on B')
            page.locator('#s-quick-mix').click()
            expect(page.locator('#s-export')).to_be_enabled(timeout=30000)
            expect(page.locator('#s-status')).to_contain_text('Preview ready')
            expect(page.locator('#s-quick-mix-row')).not_to_be_visible()
            expect(page.locator('#s-compare-state')).to_contain_text('A Before · B Live stems')
            with page.expect_download() as download:page.locator('#s-export').click()
            download.value.save_as(str(args.out/f'pending-feedback-{turn}.wav'))
            page.locator('#s-tab-mix').click()
        page.locator('#s-undo').click()
        expect(page.locator('#s-export')).to_be_enabled(timeout=30000)
        with page.expect_download() as download:page.locator('#s-export').click()
        download.value.save_as(str(args.out/'pending-feedback-undo.wav'))
        assert (args.out/'pending-feedback-1.wav').read_bytes()==(args.out/'pending-feedback-undo.wav').read_bytes()
        assert (args.out/'pending-feedback-1.wav').read_bytes()!=(args.out/'pending-feedback-2.wav').read_bytes()
        page.evaluate("document.documentElement.style.fontSize='200%'")
        page.evaluate('() => new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))')
        gain.scroll_into_view_if_needed();gain.focus();page.keyboard.press('ArrowLeft')
        expect(page.locator('#s-status')).to_contain_text('Stem controls are live on B')
        no_overflow(page)
        whole_words(page,'.edit-fader label,.edit-fader strong,#s-compare-state,#s-quick-mix-row')
        for key in ['s-play','s-stop','s-before','s-after','s-quick-mix']:
            unobscured(page,'#'+key,page.locator('.help-dock').bounding_box()['y'])
        assert page.locator('.edit-player').evaluate('e=>e.scrollHeight<=e.clientHeight+1')
        capture.capture(args.out/'pending-mix-200-active-edit.png')
        result['checks'].append('W03: first and post-download fader edits select live B and expose Update mix for saving; playback/tab switches retain pending save guards; updates render new audio and undo restores byte-identical prior export')
    finally:context.close()


def verify_cross_tab(browser, base, args, result):
    guide = json.loads((args.site/'assets/learning-guide.json').read_text(encoding='utf-8'))
    guide['steps'][0]['detail'] += ' Updated learning copy.'
    result['cross_tab'] = []

    def tab(context):
        page = context.new_page()
        diagnostics = BrowserDiagnostics(page, args.browser, result, interface_profile='fresh')
        page.on('request', lambda request: result['requests'].append({'method': request.method, 'url': request.url, 'body_bytes': len(request.post_data_buffer or b'')}))
        return page, diagnostics

    for terminal in ['completed', 'declined']:
        context = browser.new_context(viewport={'width': 1480, 'height': 940})
        held, guide_released = [], False
        try:
            first, _ = tab(context)
            first.goto(base, wait_until='networkidle')
            old, capture = tab(context)
            # Simulate a stale window that missed storage notifications. The
            # read/merge before writing must suffice without event delivery.
            old.add_init_script("window.addEventListener('storage', event=>event.stopImmediatePropagation(), true)")
            old.route('**/assets/learning-guide.json', lambda route: held.append(route))
            old.goto(base, wait_until='domcontentloaded')
            expect(old.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
            if terminal == 'completed':
                first.locator('#tutorial-start').click()
                finish_tour(first)
            else:
                first.locator('#tutorial-decline').click()
            choose_controls(old, 'advanced')
            saved = choices(old)
            assert saved['tutorial'] == terminal and saved['advanced'] is True, saved
            # Reverse the fields: an old window's tutorial navigation must not
            # undo the newer Advanced preference from the other window.
            first.locator('#open-learning').click()
            first.locator('#replay-tutorial').click()
            first.locator('#tutorial-next').click()
            first.keyboard.press('Escape')
            assert choices(first)['advanced'] is True and choices(first)['tutorial'] == terminal
            assert held
            guide_released = True
            held[0].continue_()
            expect(old.locator('#learning-dialog')).not_to_be_visible()
            capture.capture(args.out/f'cross-tab-{terminal}.png')
            newest, _ = tab(context)
            newest.route('**/assets/learning-guide.json', lambda route: route.fulfill(json=guide))
            newest.goto(base, wait_until='networkidle')
            expect(newest.locator('#learning-dialog')).not_to_be_visible()
            newest.locator('#open-learning').click()
            newest.locator('#replay-tutorial').click()
            expect(newest.locator('#learning-body')).to_contain_text('Updated learning copy.')
            newest.keyboard.press('Escape')
            newest.reload(wait_until='networkidle')
            expect(newest.locator('#learning-dialog')).not_to_be_visible()
            assert choices(newest)['tutorial'] == terminal
            assert set(newest.evaluate('Object.keys(localStorage)')) == {KEY, TERMINAL_KEY}
            result['cross_tab'].append({'terminal': terminal, 'after_stale_advanced': saved,
                'new_tab_and_reload_suppressed': True, 'content_update_preserved_choice': True})
        finally:
            if held and not guide_released:
                held[0].continue_()
                old.wait_for_load_state('networkidle')
            context.close()

    context = browser.new_context()
    try:
        first, _ = tab(context)
        old, _ = tab(context)
        first.goto(base, wait_until='networkidle')
        old.goto(base, wait_until='networkidle')
        expect(old.locator('#tutorial-start')).to_be_visible()
        first.locator('#tutorial-decline').click()
        expect(old.locator('#learning-dialog')).not_to_be_visible()
        choose_controls(old, 'advanced')
        assert choices(old)['tutorial'] == 'declined'
    finally:
        context.close()
    result['checks'].append('cross-tab completion/decline survive stale Advanced writes even with storage events blocked; tutorial writes preserve newer Advanced; new tab/reload and updated guide copy never reoffer; normal storage events close an obsolete welcome')


def run(args):
    started = time.monotonic()
    args.out.mkdir(parents=True, exist_ok=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    result = {'errors': [], 'requests': [], 'checks': [], 'layouts': []}
    base = f'http://127.0.0.1:{server.server_port}/'
    current = None
    current_capture = None
    p = browser = None
    try:
        p = sync_playwright().start()
        browser = getattr(p, args.browser).launch(headless=True, **({'executable_path': args.executable} if args.executable else {}))
        result['browser'] = args.browser + ' ' + browser.version

        def visit(seed=None, storage=None, script=None, width=1480, height=940, mobile=False, guide_override=None):
            nonlocal current, current_capture
            context = browser.new_context(viewport={'width': width, 'height': height}, storage_state=storage,
                has_touch=mobile, **({'is_mobile': mobile} if args.browser != 'firefox' else {}))
            if seed is not None:
                raw = seed if isinstance(seed, str) else json.dumps(seed)
                context.add_init_script('if (!localStorage.getItem(' + json.dumps(KEY) + ')) localStorage.setItem(' + json.dumps(KEY) + ',' + json.dumps(raw) + ');')
            if script:
                context.add_init_script(script)
            if guide_override is not None:
                context.route('**/assets/learning-guide.json', lambda route: route.fulfill(json=guide_override))
            page = context.new_page()
            current = page
            page.on('dialog', lambda dialog: dialog.accept())
            page.add_init_script('''window.learningJobs=[];const post=Worker.prototype.postMessage;
                Worker.prototype.postMessage=function(message,...args){window.learningJobs.push(message.action);return post.call(this,message,...args);};''')
            diagnostics = BrowserDiagnostics(page, args.browser, result, interface_profile='fresh')
            current_capture = diagnostics
            page.on('request', lambda request: result['requests'].append({'method': request.method, 'url': request.url, 'body_bytes': len(request.post_data_buffer or b'')}))
            page.goto(base, wait_until='networkidle')
            expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
            return context, page, diagnostics

        # Fresh welcome; focus containment, explicit dismissal and immediate save.
        context, page, capture = visit()
        expect(page.locator('#tutorial-start')).to_be_visible()
        expect(page.locator('#controls-basic')).to_have_attribute('aria-pressed', 'true')
        capture.capture(args.out/'desktop-welcome.png')
        page.locator('#tutorial-start').focus()
        page.keyboard.press('Tab')
        expect(page.locator('#learning-close')).to_be_focused()
        page.keyboard.press('Shift+Tab')
        expect(page.locator('#tutorial-start')).to_be_focused()
        for _ in range(15):
            page.keyboard.press('Tab')
            assert page.evaluate("document.querySelector('#learning-dialog').contains(document.activeElement)")
        page.keyboard.press('Escape')
        assert choices(page)['tutorial'] == 'declined'
        expect(page.locator('#open-learning')).to_be_focused()
        page.reload(wait_until='networkidle')
        expect(page.locator('#learning-dialog')).not_to_be_visible()
        declined = context.storage_state()
        context.close()
        context, page, _ = visit(storage=declined)
        expect(page.locator('#learning-dialog')).not_to_be_visible()
        context.close()
        result['checks'].append('fresh Basic profile; welcome focus trap; Escape saves decline; reload and restored context do not reoffer')

        # A started tour survives reload and can be continued from Help.
        context, page, capture = visit()
        page.locator('#tutorial-start').click()
        page.locator('#tutorial-next').click()
        page.locator('#tutorial-next').click()
        assert choices(page)['tutorial'] == 'started' and choices(page)['step'] == 2
        page.reload(wait_until='networkidle')
        expect(page.locator('#learning-dialog')).not_to_be_visible()
        page.locator('#open-learning').click()
        page.locator('#continue-tutorial').click()
        expect(page.locator('#learning-title')).to_have_text('3. Prepare a master')
        before = audio_ui(page)
        page.locator('#tutorial-back').click()
        page.locator('#tutorial-next').click()
        capture.capture(args.out/'desktop-tutorial.png')
        finish_tour(page)
        assert audio_ui(page) == before
        completed = context.storage_state()
        page.reload(wait_until='networkidle')
        expect(page.locator('#learning-dialog')).not_to_be_visible()
        context.close()
        context, page, _ = visit(storage=completed)
        expect(page.locator('#learning-dialog')).not_to_be_visible()
        page.locator('#open-learning').click()
        page.locator('#replay-tutorial').click()
        assert choices(page)['tutorial'] == 'completed'
        page.locator('#tutorial-next').click()
        page.keyboard.press('Escape')
        assert choices(page)['tutorial'] == 'completed'
        expect(page.locator('#open-learning')).to_be_focused()
        page.reload(wait_until='networkidle')
        expect(page.locator('#learning-dialog')).not_to_be_visible()
        context.close()
        result['checks'].append('started step survives reload; Continue resumes; Finish saves immediately; completion survives reload/restored context/replay dismissal; tour posts no DSP jobs or audio changes')

        for close in ['#tutorial-decline', '#learning-close']:
            context, page, _ = visit()
            page.locator(close).click()
            assert choices(page)['tutorial'] == 'declined'
            page.reload(wait_until='networkidle')
            expect(page.locator('#learning-dialog')).not_to_be_visible()
            context.close()
        context, page, _ = visit()
        page.locator('#tutorial-start').click()
        page.locator('#tutorial-dismiss').click()
        assert choices(page)['tutorial'] == 'declined'
        page.reload(wait_until='networkidle')
        expect(page.locator('#learning-dialog')).not_to_be_visible()
        context.close()
        result['checks'].append('No thanks, Close and End tour each save a terminal decline')

        for seed in [{'advanced': True}, {'version': 0, 'advanced': False, 'tutorial': 'completed', 'step': 7}]:
            context, page, _ = visit(seed=seed)
            if 'tutorial' not in seed:
                page.locator('#tutorial-decline').click()
                assert choices(page)['advanced'] is True
            else:
                expect(page.locator('#learning-dialog')).not_to_be_visible()
            context.close()
        result['checks'].append('legacy interface profile gains learning fields without losing visibility; legacy completion does not restart')

        blocked = [
            "Object.defineProperty(window,'localStorage',{get(){throw new DOMException('blocked','SecurityError')}});",
            "window.storageWrites=0;Storage.prototype.setItem=function(){window.storageWrites++;throw new DOMException('full','QuotaExceededError')};",
        ]
        for script in blocked:
            context, page, capture = visit(script=script)
            expect(page.locator('#learning-dialog')).not_to_be_visible()
            expect(page.locator('#ui-storage-notice')).to_contain_text('cannot remember')
            page.locator('#open-learning').click()
            page.locator('#replay-tutorial').click()
            while page.locator('#tutorial-next').count(): page.locator('#tutorial-next').click()
            page.locator('#tutorial-finish').click()
            page.locator('#open-learning').click()
            expect(page.locator('#learning-storage')).to_be_visible()
            capture.capture(args.out/'storage-unavailable.png')
            page.keyboard.press('Escape')
            if 'storageWrites' in script: assert page.evaluate('window.storageWrites') == 1
            page.reload(wait_until='networkidle')
            expect(page.locator('#learning-dialog')).not_to_be_visible()
            context.close()
        context, page, _ = visit(seed='{corrupt')
        expect(page.locator('#ui-storage-notice')).to_contain_text('could not be read')
        expect(page.locator('#learning-dialog')).not_to_be_visible()
        assert page.evaluate('(key)=>localStorage.getItem(key)', KEY) == '{corrupt'
        page.locator('#open-learning').click()
        page.locator('#replay-tutorial').click()
        page.keyboard.press('Escape')
        assert page.evaluate('(key)=>localStorage.getItem(key)', KEY) == '{corrupt'
        context.close()
        result['checks'].append('blocked getter, quota failure and corrupt storage: no automatic modal, transparent notice, functional Help/replay, no retry loop or corrupt-data overwrite')

        context, page, _ = visit()
        page.evaluate("() => { Storage.prototype.setItem=function(){throw new DOMException('full','QuotaExceededError')}; }")
        page.locator('#tutorial-decline').click()
        expect(page.locator('#ui-storage-notice')).to_contain_text('cannot remember')
        page.locator('#open-learning').click()
        expect(page.locator('#learning-storage')).to_be_visible()
        context.close()
        result['checks'].append('storage failure at the actual decline is caught and reported immediately')

        context, page, _ = visit(guide_override={'version': 999})
        expect(page.locator('#learning-load-notice')).to_contain_text('unavailable')
        expect(page.locator('#learning-dialog')).not_to_be_visible()
        page.locator('#open-learning').click()
        expect(page.locator('#learning-title')).to_have_text('Guidance unavailable')
        context.unroute('**/assets/learning-guide.json')
        page.locator('#learning-retry').click()
        expect(page.locator('#replay-tutorial')).to_be_visible()
        expect(page.locator('#learning-load-notice')).not_to_be_visible()
        context.close()
        result['checks'].append('unsupported guidance data fails visibly without an automatic modal; explicit Retry loads the shared guide and clears the old notice')

        # Real short generated audio proves disclosure and guidance preserve a preview.
        context, page, capture = visit(storage=declined)
        audio_supported = page.evaluate('Boolean(globalThis.AudioContext || globalThis.webkitAudioContext)')
        result['audio_supported'] = audio_supported
        if audio_supported:
            page.locator('#file').set_input_files(fixture('learning-generated.wav', seconds=8))
            expect(page.locator('#master')).to_be_enabled(timeout=30000)
            page.locator('#master').click()
            expect(page.locator('#export')).to_be_enabled(timeout=120000)
            capture.capture(args.out/'desktop-basic-preview.png')
            page.locator('#play').click()
            expect(page.locator('#play')).to_have_text('Pause', timeout=30000)
            playing = audio_ui(page)
            page.locator('#open-learning').click()
            page.locator('#learning-topics').click()
            page.locator('#help-mcp').click()
            page.locator('#learning-topics').click()
            page.locator('#help-glossary').click()
            page.keyboard.type('LUFS')
            expect(page.locator('#glossary-search')).to_be_focused()
            page.locator('#learning-topics').click()
            page.locator('#help-quickstart').click()
            page.locator('#replay-tutorial').click()
            page.locator('#tutorial-next').click()
            page.keyboard.press('Escape')
            assert audio_ui(page) == playing
            expect(page.locator('#play')).to_have_text('Pause')
            page.locator('#stop').click()
            result['checks'].append('Help and tutorial navigation during actual preview playback preserve playing state, source, listening controls, parameters and worker requests')
        else:
            result['skipped'] = ['Real audio import/preview/playback, pending-job cancellation and voice-error recovery: neither AudioContext nor webkitAudioContext exists in this installed browser. No audio APIs are mocked.']
            capture.capture(args.out/'desktop-basic-empty.png')
        choose_controls(page, 'advanced')
        page.locator('#custom-target').check()
        page.locator('#target').fill('-16')
        page.locator('#ceiling').fill('-2')
        page.locator('#ceiling').dispatch_event('input')
        page.locator('#intensity').fill('0.8')
        page.locator('#intensity').dispatch_event('input')
        choose_page(page, 'eq')
        page.locator('[data-band="0"][data-parameter="q"]').fill('1.3')
        page.wait_for_timeout(150)
        before = audio_ui(page)
        choose_controls(page, 'basic')
        expect(page.locator('#advanced-notice')).to_contain_text('target -16 LUFS')
        expect(page.locator('#advanced-notice')).to_contain_text('custom EQ band')
        expect(page.locator('[data-band="0"][data-parameter="q"]')).not_to_be_visible()
        assert audio_ui(page) == before
        page.locator('#open-learning').click()
        page.locator('#replay-tutorial').click()
        finish_tour(page)
        assert audio_ui(page) == before
        choose_controls(page, 'advanced')
        expect(page.locator('#target')).to_have_value('-16')
        expect(page.locator('#ceiling')).to_have_value('-2')
        expect(page.locator('[data-band="0"][data-parameter="q"]')).to_have_value('1.3')
        capture.capture(args.out/'desktop-advanced-eq.png')
        result['checks'].append(('real audio preview' if audio_supported else 'UI without Web Audio') + ': Basic/Advanced preserves target, ceiling, intensity and EQ shape; hidden active settings are explicit; tutorial preserves parameters and worker requests')

        for width, height in [(320, 740), (390, 844), (430, 932)]:
            page.set_viewport_size({'width': width, 'height': height})
            choose_controls(page, 'basic')
            choose_page(page, 'overview')
            page.evaluate('scrollTo(0,0)')
            no_overflow(page)
            capture.capture(args.out/f'phone-{width}-basic.png')
            dock, help_button, transport = [page.locator(selector).bounding_box() for selector in ['.help-dock', '#open-learning', '.playback']]
            transport = transport or {'y':dock['y'],'height':0}
            assert help_button['width'] >= 44 and help_button['height'] >= 44
            assert transport['y'] >= 0 and transport['y'] + transport['height'] <= dock['y'] + 1
            page.locator('#format').scroll_into_view_if_needed()
            box = page.locator('#format').bounding_box()
            assert box['y'] >= 0 and box['y'] + box['height'] <= transport['y'] + 1
            capture.capture(args.out/f'phone-{width}-export-settings.png')
            page.locator('#open-learning').click()
            page.locator('#help-mcp').click()
            expect(page.locator('#learning-body')).to_contain_text('Desktop companion feature')
            no_overflow(page)
            capture.capture(args.out/f'phone-{width}-help-mcp.png')
            page.locator('#help-glossary').click()
            page.locator('#glossary-search').fill('LUFS')
            expect(page.locator('.learning-glossary dt')).to_have_text('LUFS')
            page.keyboard.press('Escape')
            expect(page.locator('#open-learning')).to_be_focused()
            choose_page(page, 'studio-tools')
            page.evaluate('scrollTo(0,0)')
            expect(page.locator('.library')).not_to_be_visible()
            no_overflow(page)
            studio_player = page.locator('.edit-player').bounding_box() or {'y':dock['y'],'height':0}
            assert studio_player['y'] + studio_player['height'] <= dock['y'] + 1
            unobscured(page, '#s-source', studio_player['y'])
            capture.capture(args.out/f'phone-{width}-studio.png')
            choose_page(page, 'overview')
            page.evaluate("document.documentElement.style.fontSize='200%'")
            no_overflow(page)
            page.locator('#format').scroll_into_view_if_needed()
            capture.capture(args.out/f'phone-{width}-text-200.png')
            page.locator('#open-learning').click()
            page.locator('#help-mcp').click()
            no_overflow(page)
            assert page.locator('#learning-dialog').evaluate('el=>el.scrollWidth<=el.clientWidth')
            capture.capture(args.out/f'phone-{width}-help-text-200.png')
            page.keyboard.press('Escape')
            page.evaluate("document.documentElement.style.fontSize=''")
            result['layouts'].append({'width': width, 'height': height, 'text_percent': [100, 200], 'help_target': help_button, 'playback': transport, 'help_dock': dock})
        page.set_viewport_size({'width': 1100, 'height': 720})
        choose_page(page, 'eq')
        capture.capture(args.out/'desktop-1100x720-eq-top.png')
        page.locator('#band-picker button').nth(2).click()
        page.locator('[data-band="2"][data-parameter="gain"]').click()
        compact_player = page.locator('.playback').bounding_box()
        unobscured(page, '[data-band="2"][data-parameter="gain"]', compact_player['y'] if compact_player else page.locator('.help-dock').bounding_box()['y'])
        no_overflow(page)
        capture.capture(args.out/'desktop-1100x720-eq-controls.png')
        result['checks'].append('1100×720 desktop uses document scrolling; selected EQ gain can be hit normally above pinned playback')
        page.set_viewport_size({'width': 1480, 'height': 940})
        choose_controls(page, 'advanced')
        page.reload(wait_until='networkidle')
        expect(page.locator('#controls-advanced')).to_have_attribute('aria-pressed', 'true')
        stored = page.evaluate('Object.fromEntries(Object.entries(localStorage))')
        assert set(stored) == {KEY, TERMINAL_KEY} and len(stored[KEY]) < 1024
        assert stored[TERMINAL_KEY] in ['completed', 'declined']
        assert set(json.loads(stored[KEY])) == {'version', 'advanced', 'tutorial', 'step'}
        result['checks'].append('320/390/430 phones: Help and transport do not overlap; export format reachable; MCP desktop-only guidance; glossary search; Studio; 200% text without horizontal overflow')
        result['checks'].append('reload remembers four small interface fields and a terminal-only marker; no persistent audio or DSP settings')
        context.close()

        # Baseline B01/B02/B03: genuine mobile viewport and ordinary hit tests.
        context, page, capture = visit(storage=declined, width=320, height=844, mobile=True)
        no_overflow(page)
        choose_page(page, 'studio-tools', touch=True)
        expect(page.locator('#studio-tools')).to_be_visible()
        page.locator('#tab-eq').focus()
        page.keyboard.press('Enter')
        page.locator('#band-picker button').nth(2).tap()
        expect(page.locator('.band.active .band-name')).to_contain_text('Warmth')
        no_overflow(page)
        choose_page(page, 'overview')
        if audio_supported:
            page.locator('#file').set_input_files(fixture('mobile-recovery.wav', seconds=4))
            expect(page.locator('#mobile-master')).to_be_enabled(timeout=30000)
            hold_worker_result(page, 'dsp-client.js', 'DspClient', 'master')
            page.locator('#mobile-master').tap()
            expect(page.locator('#job-cancel-master')).to_be_visible()
            page.wait_for_function('() => typeof window.releaseLearningJob === "function"')
            unobscured(page, '#job-cancel-master', page.locator('.playback').bounding_box()['y'])
            capture.capture(args.out/'phone-320-master-cancel.png')
            page.locator('#job-cancel-master').tap()
            expect(page.locator('#status')).to_contain_text('Cancelled. Your original is ready', timeout=30000)
            expect(page.locator('#export')).to_be_disabled()
            page.evaluate('window.releaseLearningJob()')
            choose_page(page, 'studio-tools', touch=True)
            page.locator('#s-use-main').tap()
            expect(page.locator('#s-export')).to_be_enabled(timeout=30000)
            choose_controls(page, 'advanced')
            page.locator('#s-tab-voice').click()
            page.locator('#s-highpass').fill('5')
            choose_controls(page, 'basic')
            page.locator('#s-voice').tap()
            expect(page.locator('#s-field-error')).to_contain_text('20–300', timeout=30000)
            expect(page.locator('#s-action-error')).to_contain_text('20–300')
            expect(page.locator('#s-highpass')).to_have_attribute('aria-invalid', 'true')
            expect(page.locator('#s-highpass')).to_be_focused()
            unobscured(page, '#s-field-error', page.locator('.edit-player').bounding_box()['y'])
            capture.capture(args.out/'phone-320-local-voice-error.png')
            page.locator('#s-highpass').fill('75')
            hold_worker_result(page, 'studio-client.js', 'StudioClient', 'voice')
            page.locator('#s-voice').tap()
            page.wait_for_function('() => typeof window.releaseLearningJob === "function"')
            expect(page.locator('#job-cancel-studio')).to_be_enabled()
            unobscured(page, '#job-cancel-studio', page.locator('.edit-player').bounding_box()['y'])
            capture.capture(args.out/'phone-320-studio-cancel.png')
            page.locator('#job-cancel-studio').tap()
            page.evaluate('window.releaseLearningJob()')
            expect(page.locator('#s-status')).to_contain_text('Cancelled', timeout=30000)
            expect(page.locator('#s-export')).to_be_enabled(timeout=30000)
            page.locator('#s-voice').tap()
            expect(page.locator('#s-export')).to_be_enabled(timeout=30000)
            expect(page.locator('#s-status')).to_contain_text('Preview ready')
            expect(page.locator('#s-action-error')).not_to_be_visible()
            no_overflow(page)
        context.close()
        result['checks'].append('320 mobile viewport: normal Studio/EQ/band touch hits and keyboard activation')
        if audio_supported:
            result['checks'].append('Visible main/Studio Cancel buttons over real pending jobs; 5 Hz error is local and focused; 75 Hz recovery succeeds')
        verify_cross_tab(browser, base, args, result)
        verify_interleaved_terminal(browser, base, args, result)
        verify_terminal_storage_failures(browser, base, args, result)
        verify_review_layouts(browser, base, args, result, audio_supported)
        verify_help_reachability(browser, base, args, result)
        if audio_supported:verify_pending_feedback(browser, base, args, result)
        assert all(request['method'] == 'GET' and request['body_bytes'] == 0 and request['url'].startswith(base) for request in result['requests'])
        runtime = [urlparse(request['url']) for request in result['requests'] if Path(urlparse(request['url']).path).suffix in {'.js', '.css', '.wasm'}]
        versions = {tuple(parse_qs(url.query).get('v', [])) for url in runtime}
        assert len(versions) == 1 and len(next(iter(versions))) == 1, versions
        result['runtime_version'] = next(iter(versions))[0]
        assert not result['errors'], result['errors']
        result['success'] = True
    except Exception as error:
        result['success'] = False
        result['failure'] = str(error)
        if current and not current.is_closed():
            try:
                current_capture.capture(args.out/'failure.png', full_page=True)
            except Exception as capture_error:
                result['failure_capture'] = str(capture_error)
        raise
    finally:
        if browser: browser.close()
        if p: p.stop()
        server.shutdown()
        server.server_close()
        thread.join()
        result['elapsed_seconds'] = round(time.monotonic() - started, 2)
        (args.out/'learning-verification.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--site', type=Path, default=ROOT/'web/dist')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    parser.add_argument('--executable')
    args = parser.parse_args()
    run(args)
    report = json.loads((args.out/'learning-verification.json').read_text(encoding='utf-8'))
    summary={key:value for key,value in report.items() if key in ['browser','runtime_version','success','audio_supported','elapsed_seconds','checks','errors','interleaved_terminal','skipped']}
    summary['review_layout_cases']=len(report.get('review_layouts',[]))
    summary['automation_diagnostic_count']=len(report.get('automation_diagnostics',[]))
    print(json.dumps(summary,indent=2))
