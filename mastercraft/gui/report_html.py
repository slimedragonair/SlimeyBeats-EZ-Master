"""HTML rendering of analysis / mastering reports for the report view."""
import html


def _sev_color(s):
    return "#3c8" if s < 15 else "#bc4" if s < 40 else "#e93" if s < 70 else "#e44"


def _bar(s):
    c = _sev_color(s)
    return (f"<table cellspacing=0 cellpadding=0 width=120 style='background:#333'><tr>"
            f"<td width={max(1, int(s * 1.2))} style='background:{c}'>&nbsp;</td><td></td></tr></table>")


def metrics_table(inp, out=None, targets=None):
    keys = [("integrated_lufs", "Integrated", "LUFS"), ("true_peak_dbtp", "True peak", "dBTP"),
            ("sample_peak_dbfs", "Sample peak", "dBFS"), ("lra_lu", "Loudness range (LRA)", "LU"),
            ("plr_db", "PLR (peak-to-loudness)", "dB"), ("psr_db", "PSR (peak-to-short-term)", "dB"),
            ("short_term_max_lufs", "Short-term max", "LUFS"), ("momentary_max_lufs", "Momentary max", "LUFS"),
            ("crest_factor_db", "Crest factor", "dB")]
    h = "<table cellpadding=3 style='border-collapse:collapse'><tr><th align=left>Metric</th><th>Before</th>"
    h += "<th>After</th>" if out else ""
    h += "</tr>"
    for k, lbl, u in keys:
        a = inp["metrics"].get(k)
        h += f"<tr><td>{lbl}</td><td align=right>{a:.2f} {u}</td>"
        if out:
            b = out["metrics"].get(k)
            h += f"<td align=right><b>{b:.2f}</b> {u}</td>"
        h += "</tr>"
    if out and targets:
        h += f"<tr><td colspan=3 style='color:#8ad'>Target {targets['lufs']:.1f} LUFS / {targets['ceiling_dbtp']:.1f} dBTP ({targets['mode']})</td></tr>"
    return h + "</table>"


def issues_table(rep, other=None):
    oth = {i["key"]: i["severity"] for i in (other or {}).get("issues", [])}
    h = "<table cellpadding=4 width=100%>"
    for i in rep["issues"]:
        s = i["severity"]
        after = f"<br><small>after: {oth[i['key']]:.0f}</small>" if i["key"] in oth else ""
        h += (f"<tr><td valign=top width=130>{_bar(s)}<small style='color:{_sev_color(s)}'>{s:.0f}/100 · {i['level']}</small>{after}</td>"
              f"<td valign=top><b>{html.escape(i['title'])}</b><br>{html.escape(i['explanation'])}"
              f"<br><small style='color:#9ab'>Fix: {html.escape(i['fix'])}</small></td></tr>")
    return h + "</table>"


def render(payload):
    inp = payload.get("input")
    if not inp:
        return "<p>No report yet.</p>"
    out = payload.get("output")
    h = "<html><body style='font-family:Segoe UI; font-size:10pt; color:#ddd; background:#1b1d22'>"
    h += f"<h3>{html.escape(str(payload.get('path', '')))}</h3>"
    h += f"<p>{inp['sr']} Hz · {inp['channels']} ch · {inp['duration_s']:.1f} s · profile <b>{html.escape(str(payload.get('profile') or inp.get('profile')))}</b>"
    if inp.get("genre_suggestion"):
        h += " · closest genre curve: " + ", ".join(f"{g} ({d:.1f})" for g, d in inp["genre_suggestion"])
    h += "</p>"
    bl = inp.get("blend_suggestion")
    if bl and bl.get("spec") and len(bl.get("items", [])) > 1:
        h += (f"<p style='color:#8ad'>Closest <b>blend</b> of two curves: {html.escape(bl['display_name'])} "
              f"&rarr; <code>-p \"{html.escape(bl['spec'])}\"</code></p>")
    notes = payload.get("notes") or inp.get("warnings") or []
    for n in notes:
        h += f"<p style='color:#fb5'>⚠ {html.escape(n)}</p>"
    if payload.get("out_path"):
        h += f"<p>Output: <b>{html.escape(payload['out_path'])}</b></p>"
    h += "<h4>Loudness &amp; dynamics</h4>" + metrics_table(inp, out, payload.get("targets"))
    h += "<h4>Detected issues (input)</h4>" + issues_table(inp, out)
    if payload.get("plan"):
        h += "<h4>Processing chain</h4><ol>"
        logs = {l["module"]: l for l in payload.get("log", [])}
        for s in payload["plan"]["steps"]:
            extra = ""
            if s["module"] == "static_eq":
                extra = "<br><small>" + "<br>".join(
                    f"{f['type']} {f['freq_hz']:.0f} Hz {f['gain_db']:+.1f} dB (Q {f['q']})"
                    + (f" &mdash; {html.escape(f['why'])}" if f.get("why") else "") for f in s["filters"]) + "</small>"
                t = s.get("tilt") or {}
                if t.get("applied_db"):
                    extra += (f"<br><small style='color:#9ab'>Tilt {t['slope_db_per_oct']:+.2f} dB/oct corrected "
                              "with two shelves first</small>")
                lm = s.get("lowmid_response_db") or {}
                if lm:
                    extra += ("<br><small style='color:#9ab'>EQ response at " +
                              ", ".join(f"{k} Hz {v:+.1f} dB" for k, v in lm.items()) + "</small>")
                for g in s.get("lowmid_guard") or []:
                    extra += (f"<br><small style='color:#fb5'>Kick-body guard: low cuts scaled x{g['scale']} "
                              f"({g['over_by_db']:.1f} dB too much at {g['freq_hz']:.0f} Hz)</small>")
            info = logs.get(s["module"], {}).get("info")
            if info:
                extra += f"<br><small style='color:#999'>{html.escape(str(info))[:400]}</small>"
            h += f"<li><b>{s['module']}</b>: {html.escape(s['reason'])}{extra}</li>"
        h += "</ol>"
        pu = payload.get("punch")
        if pu:
            h += ("<h4>Punch / transients (kick &amp; snare hits)</h4><table cellpadding=3><tr><td>view</td><td>hits</td>"
                  "<td>TSR in dB</td><td>TSR loss median</td><td>TSR loss p90</td><td>% smeared</td><td>hit-level spread p90</td></tr>")
            for k, c in pu["vs_input"].items():
                if not c.get("hits"):
                    continue
                bad = c.get("tsr_loss_db_p90", 0) > pu["threshold_db"]
                h += (f"<tr><td>{k}</td><td>{c['hits']}</td><td>{pu['input'].get(k, {}).get('tsr_db', '')}</td>"
                      f"<td>{c['tsr_loss_db_median']:+.2f}</td><td style='color:{'#e93' if bad else '#9a9'}'>{c['tsr_loss_db_p90']:+.2f}</td>"
                      f"<td>{100 * c['frac_hits_smeared']:.1f}</td><td>{c.get('hit_level_spread_db', '')}</td></tr>")
            h += f"</table><p><small>Punch score {pu['score_db']:.2f} dB (limit {pu['threshold_db']:.1f} dB). "
            secs = [f"{k} {int(s['start_s'] // 60)}:{s['start_s'] % 60:04.1f}-{int(s['end_s'] // 60)}:{s['end_s'] % 60:04.1f}"
                    for k, c in pu["vs_input"].items() for s in c.get("sections", [])[:6]]
            h += ("Sections with smeared hits: " + html.escape(", ".join(secs)) if secs else "No section with smeared hits.") + "</small></p>"
            for g in pu.get("guard", []):
                if g.get("action"):
                    h += f"<p><small>Punch guard round {g['round']}: {html.escape(g['reason'])} &rarr; {html.escape(g['action'])}</small></p>"
        sf = payload.get("safety", {})
        if sf:
            h += f"<p><small>Safety check: {len(sf['attempts'])} attempt(s), chose {100 * sf['chosen_intensity_scale']:.0f}% intensity.</small></p>"
    t = inp.get("tonal")
    if t:
        h += "<h4>Tonal balance (octave bands, dB re 1 kHz)</h4><table cellpadding=3><tr><td>Hz</td>" + \
             "".join(f"<td align=right>{b:g}</td>" for b in t["bands_hz"]) + "</tr><tr><td>Measured</td>" + \
             "".join(f"<td align=right>{v:+.1f}</td>" for v in t["measured_db"]) + "</tr><tr><td>Target</td>" + \
             "".join(f"<td align=right>{v:+.1f}</td>" for v in t["target_db"]) + "</tr><tr><td>Deviation</td>" + \
             "".join(f"<td align=right style='color:{'#e93' if abs(v) > 2 else '#9a9'}'>{v:+.1f}</td>" for v in t["deviation_db"]) + "</tr></table>"
    st = inp.get("stereo")
    if st:
        h += (f"<h4>Stereo</h4><p>Correlation {st['corr']:.2f}, side/mid {st['sm_db']:.1f} dB, mono fold loss {st['mono_loss_lu']:.2f} LU, "
              f"balance {st['balance_db']:+.2f} dB<br>Bands: " +
              ", ".join(f"{k} r={v['corr']:.2f} S/M {v['sm_db']:.1f} dB" for k, v in st["bands"].items()) + "</p>")
    return h + "</body></html>"
