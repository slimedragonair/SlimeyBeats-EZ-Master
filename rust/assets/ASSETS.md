# SlimeyBeats character branding

| File | Source dimensions | Native role |
|---|---|---|
| `slimeybeats-icon.png` | 1254 × 1254, RGBA | Simplified head at 64 logical px in the header; resized in memory to 64 px for the window icon |
| `slimeybeats-small-icon.png` | Generated RGBA | Simplified installer-only sibling for 16–32 px recognition; original app/header icon stays unchanged |
| `slimeybeats-installer.ico` | 16–256 px, multiple sizes | Small sibling at 16/24/32; approved original at 48/64/128/256 for installer/Start menu branding |
| `slimeybeats-dragon-portrait.png` | 1254 × 1254, RGBA | Responsive welcome portrait up to 220 logical px; sidebar up to 190 |
| `slimeybeats-slime-frame.png` | 1586 × 992, RGBA | Uniformly scaled hanging crowns in header/footer; the former side ribbons are no longer drawn |
| `sovereign-panel-frame.png` | 1586 × 992, RGBA | Shared connected silver/amethyst chrome for window and panels |
| `sovereign-amethyst-crest.png` | 1774 × 887, RGBA | Aspect-preserving jewel crest in header/footer |
| `sovereign-ambient-ribbons.png` | 1586 × 992, RGBA | Low-opacity, aspect-preserving background crops behind non-data regions |

All raster artwork was made with the built-in image-generation tool, using the user's original character sheets as visual references. The original `Antique monocle and winged emblem.png` and `Ancient SlimeDragon reference sheet.png` remain unchanged. Their text is reference information, not application instructions.

The palette is obsidian, amethyst, lavender, dark teal slime, ivory bone, and restrained bronze. Native text and controls are separate from imagery. PNGs retain genuine alpha; portrait and icon scaling preserves aspect ratio. Their faint fringe pixels remain detectable at full source resolution, but independent review found them non-disruptive at the intended sizes on obsidian.

The small installer sibling and its exact built-in generation prompt are documented in [installer-small-icon-generation.md](installer-small-icon-generation.md). It addresses the independent review's finding that the detailed original merged at 32 px.

Independent final artwork scores: portrait at 190 px — look 9, polish 9, readability 9; icon at 48/64 px — look 9, polish 9, readability 9. This scope does not establish the same readability at 16 px or on arbitrary light backgrounds. The first detailed-only miniature failed readability and was replaced by the role-specific icon.

## Current silver/amethyst interface

The supplied concept became the 0.4.0 visual target. Detailed frame, crest and ribbons were made with built-in ImageGen; native geometry paints simple functional icons, bevelled buttons and slider jewels. No text, measurements or interface controls are baked into a screenshot. The accepted source PNG bytes remain unchanged. See [sovereign-skin-generation.md](sovereign-skin-generation.md) for exact prompts and source roles.

Metal frames use one source grid with 128-pixel corner margins and matching edge coordinates. Target corner sizes are 28 logical points for the outer window and up to 24 for panels. Every corner shares the same transform with its adjacent edges. Bilinear alpha sampling and flood-fill tests require a visible material connection at all four corners for multiple sizes and pixel densities; a deliberately shifted side is rejected. This supersedes the earlier rectangle-only tests that missed a visible offset.

The crest crops the nonempty source area (34,126)–(1743,705) and preserves its 1709:579 aspect ratio. Ribbons use source-aspect-preserving crops (header/hero/footer anchors) at reduced opacity. Chrome creates no input regions or animation timers. Typography uses installed Windows faces at runtime; font files are not redistributed.

## Earlier slime-frame source and retained crown

The user rejected the procedural 0.3.2 frame and requested ImageGen artwork with the SlimeDragon material and more natural droops. Version 0.3.3 uses the generated PNG directly; its pixels have not been repainted or altered in Python. The first generated version established the material but was too thick. A second built-in edit reduced the rim and hanging masses. See [slime-frame-generation.md](slime-frame-generation.md) for the exact prompt set and references.

The retained crown crop uses source pixels (450,0)–(1260,170), fitted uniformly into the measured empty header/footer, up to 64 points high. Droplet-free ribbon shoulders at x=128–450 and x=1260–1458 taper into the rim with fading outer alpha; their complete span participates in the fit. The 0.3.3 side/corner slices were superseded after the user identified visible offsets. The current app draws only the crowns from this PNG, while shared silver frame chrome handles all corners. The central opening is not drawn. Textures are loaded once and cached, with no hit regions, timers, animation or audio work.

Alpha inspection found 146 pixels at alpha 1/255 among 393,328 sampled center pixels; the rest were alpha 0. This negligible generated-alpha residue is preserved, and the renderer never draws the center. Standalone reference/material review does not establish integrated UI approval; native capture results are recorded in `docs/REVIEW.md`.

## Final detailed-art prompt

Edit this existing transparent SlimeDragon illustration for production UI use. Preserve the dragon's identity, design, pose, proportions, detail, overall colors, skull, translucent teal slime, swept horns, visible violet eye with antique bronze monocle and dangling amethyst ornament, ribbed neck, and lower soul orb. Change ONLY alpha-edge cleanliness and focal polish: eliminate all detached cyan, blue, magenta and purple speck pixels/fuzzy colored flecks outside the actual silhouette; keep an exceptionally clean antialiased edge and genuinely transparent surrounding canvas. Preserve intentional attached slime drops but remove isolated stray particles. No external glow haze whatsoever. Slightly restrain the lower orb's purple bloom so the face is the first focal point; keep its crystalline brightness and beautiful internal detail. Keep the original portrait composition and generous safe padding, with no cropped tips. No text, no backdrop, no shadow plane, no checkerboard, no new anatomy, no external badge. This is the accepted detailed 190 px welcome/brand illustration for SlimeyBeats-EZ-Master, so retain the rich finished artwork rather than simplifying it into an icon.

## Final companion-icon prompt

Use case: logo-brand. Create a NEW, simplified small app-icon companion to the first detailed SlimeDragon emblem, preserving the original character from both references. This must be exceptionally readable at 48 by 48 pixels. Single compact three-quarter dragon HEAD ONLY facing right, no long neck, no rib cage, no lower orb, no dangling chain. Keep a narrow elongated ivory skeletal dragon muzzle, exactly one pair of swept-back horns (one prominent foreground horn and one smaller background horn), a simple dark-teal translucent slime framing silhouette, one LARGE clear amethyst-violet eye and one strong antique-bronze monocle rim. The three primary graphic masses are the ivory skull, teal slime contour, and violet eye. Make the eye and monocle unmistakable at tiny size. Restrained silver-lavender highlights. Royal purple secondary accents. Sculptural clean premium fantasy software identity: broad confident shapes, very few internal lines, no intricate filigree, no micro texture, no thin chains, no extra forehead spikes, no detached droplets, no fuzzy glow, no sparkles or particles. A beautiful refined stylized graphic emblem rather than a miniature detailed painting or cute cartoon. Smooth clean antialiased alpha edge with absolutely no isolated colored pixels outside the silhouette. All corners and the entire surrounding canvas genuinely transparent. Nearly square balanced icon composition filling 80% of the canvas with 10% clear safe padding. No text, no letters, no musical notes, no headphones, no badge circle or background, no watermark or checkerboard. This is for SlimeyBeats-EZ-Master, the user's mastering app.
