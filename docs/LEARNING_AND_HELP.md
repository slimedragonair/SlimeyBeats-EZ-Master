# Learning and Help

**In-development UX/learning preview:** these changes are on `feature/ux-learning`, pending review and not merged or released. They do not update public v0.4.2 stable or published v0.5.0-beta.1 Preview. Desktop source still identifies as 0.5.0-beta.3; a new release version has not been decided. Existing screenshots and beta qualification reports describe their named candidates, not acceptance of this pass.

For your first file, follow [the mastering workflow](USER_GUIDE.md). For individual parts, use [Studio](STUDIO.md). This page covers the optional guidance and interface choices.

## Basic and Advanced

**Basic is the default.** It keeps common actions in view. **Advanced** reveals precise targets and less frequent controls. Switching modes or pages changes visibility; it never resets or silently replaces your audio settings. A custom-setting notice identifies hidden values that still affect processing. Choose Advanced to inspect them.

Analyze measures audio. Master or a Studio Preview action prepares audio to compare. Export saves the prepared version. In Studio, **Update mix preview** renders pending faders; **Apply preview to selected stem** puts a prepared stem edit back into the mix. Changing visibility does none of these actions. See [the Studio workflow](STUDIO.md#desktop-workflow).

## Take the quick tour

A new interface profile gets an optional eight-step introduction:

1. Add your song, or choose the example yourself.
2. Understand the mix with Analyze.
3. Choose a direction and prepare a master.
4. Listen and compare A/B.
5. Shape the tone with Manual EQ.
6. Export when you are ready.
7. Explore Studio for stems, cleanup and repair.
8. Find Help and continue exploring.

Advancing the tour never loads audio, runs processing, downloads a model or exports a file. On desktop, **See this screen** pauses the tour and opens the relevant workspace. **Tour** / **Continue tour** in the bottom bar returns to it. Browser steps tell you where to find each task.

Finishing, declining the offer or dismissing the offer/tutorial saves that choice immediately, without waiting for **Save settings** or normal exit. It suppresses later automatic offers when storage succeeds. Replay remains available through Help and does not re-enable an automatic offer. An unfinished first browser tour can be continued from Help after reload.

## Open Help

Use **Help** in the bottom-right corner. On desktop, **F1** opens it too. Quick start includes tour replay; Terms/Glossary explains unfamiliar words; troubleshooting covers playback, imports and unavailable exports. Desktop Help also covers Studio and AI setup. Use the visible close action or Escape to dismiss a dialog.

Help and the tour preserve your audio session. Closing a tutorial records dismissal; closing Help simply returns to your work. You can use the app without an AI account or connection.

## Connect a desktop AI client

In the desktop preview, **Help → AI setup** can generate a connection example when the MCP companion is present. Choose the audio and export folders, generate the JSON, then copy it into your compatible client's supported MCP setup. The helper uses the real companion's `--print-config` output. It does not edit an AI client's settings or start an MCP server connection. Your client launches the configured local stdio companion when you connect it.

See [the in-app setup steps](MCP.md#in-app-setup-preview) for button names, missing-companion recovery and privacy. Browser **AI / MCP setup** is desktop-only guidance: the web app has no upload or localhost bridge to the companion. Use [the desktop connection guide](MCP.md#set-up-a-connection); no browser connection is required for audio editing.

## Remembered choices

| Surface | What is remembered | Where |
|---|---|---|
| Desktop interface | Basic/Advanced, tutorial status and position | `interface-preferences.json`, separate from audio/desktop preferences |
| Desktop tutorial history | A completed, declined or dismissed automatic offer | `tutorial-dismissed.marker`, beside the interface file |
| Browser interface | Small mode and tutorial choices | This site's `localStorage`; no audio or processing settings |

Desktop interface files use the existing settings directory: portable `user_data/` on Windows, and the existing [XDG config location on Linux](LINUX.md#user-files-and-desktop-integration). They do not relocate `rust-settings.json` or `desktop-preferences.json`. The tutorial marker is a one-way latch: an older open window cannot restore an automatic offer after another window has completed or dismissed it. Help replay still works.

Interface preferences do not restore audio, the queue or prepared previews. Keep desktop Studio work with **Save project**; export audio explicitly. [Audio and desktop preference details](USER_GUIDE.md#remembering-desktop-settings) remain separate.

If native interface preferences cannot be read, the file is preserved and an error is shown. Help offers **Save current interface preferences** for explicit recovery. Write failures also show an error; do not assume a choice survived a restart when saving failed.

If browser storage is blocked, unavailable or unreadable, a visible notice explains that choices last only for the current tab. Automatic guidance stays off; Help and manual replay remain available. Reloading or closing a browser tab discards its audio session even when interface storage works. If guidance itself cannot load, open Help to retry.

## Contributor map

Steps, glossary and MCP copy share one source: [`rust/assets/learning-guide.json`](../rust/assets/learning-guide.json). Native embeds it; the web build copies it to `assets/learning-guide.json`. Edit the shared source rather than maintaining a second tour. Controls and actions stay native/DOM elements, separate from that copy.

| Responsibility | Owning files |
|---|---|
| Native mode/tutorial state and storage | [`user_experience.rs`](../rust/src/gui/user_experience.rs), [`interface_preferences.rs`](../rust/src/gui/interface_preferences.rs), [`tutorial_history.rs`](../rust/src/gui/tutorial_history.rs), [`preference_store.rs`](../rust/src/gui/preference_store.rs); paths in [`paths.rs`](../rust/src/paths.rs) |
| Native guidance and connection example | [`learning_ui.rs`](../rust/src/gui/learning_ui.rs), [`help_center.rs`](../rust/src/gui/help_center.rs), [`guide_content.rs`](../rust/src/gui/guide_content.rs), [`mcp_setup.rs`](../rust/src/gui/mcp_setup.rs) |
| Browser choices, visibility and dialogs | [`ui-preferences.js`](../web/site/ui-preferences.js), [`control-visibility.js`](../web/site/control-visibility.js), [`learning.js`](../web/site/learning.js), [`learning-content.js`](../web/site/learning-content.js), [`dialog-focus.js`](../web/site/dialog-focus.js) |
| Shared-copy web packaging | [`web/tools/build.py`](../web/tools/build.py) |

Keep preference writes separate from audio operations, retain terminal tutorial choices across stale windows/tabs, and keep MCP setup explicit. The final qualification matrix belongs to the reviewed candidate; this document makes no new test-count, visual-score or physical-device claim.
