# D3 — Patchbay

**Identity:** a studio patchbay / mixing console. Sources are hardware modules with their own color, entities are jacks, and a cross-source link is a *patch cable* between two jacks. Light industrial gray metal, bold flat hardware controls, engraved labels. Tactile but flat — no fake 3D.

## Tokens
- Chassis `#E3E5E8` · Panel `#F1F2F4` · Panel edge `#C9CDD3` · Engraved label `#1E2126` · Muted `#6B727C`
- **Per-source primaries** (the only saturated colors, used for module headers, jacks, cables, badges):
  - Jira `#2563EB` blue · Confluence `#7C3AED` violet · Gitea `#2E9E5B` green · TeamCity `#F0742A` orange · Local notes `#E0B100` yellow
- Status: failed `#D62839`, running = the source color pulsing ring, success = dark engraved check. Selection: a black 2 px "patched" outline.

## Type (Google Fonts)
- Display / module names: **Space Grotesk** 600–700
- Body & UI: **IBM Plex Sans** 400/500
- Labels, keys, jack numbers: **Space Mono** 400, uppercase for engraved plate labels at 10–11 px with +0.12em tracking.

## Shape & density
- 4 px radii on panels, 999 px on jacks/knobs. Panels have a 1 px edge and four tiny screw-head dots in the corners (inline SVG, subtle). Controls are chunky: 28 px toggles with a visible thumb, segmented switches, a big round *Start/Stop* timer button like a transport control.
- Medium density; modules have headers in their source color.

## Signature
**Patch cables.** Links between entities are drawn as cables: SVG cubic Bézier curves in the *source color of the origin*, 3 px, with a round jack at each end and a slight sag (control points below the line). Suggested links are the same cable at 40 % opacity and dashed; *Confirm* makes it solid with a short "plug-in" animation (scale on the jack). On the entity detail, the links panel draws cables from the ticket's jack column to the linked items' jacks. In list views, a colored jack dot marks each row's source. If the paradigm has a graph, the edges are cables.

## Motion
Jack plug-in (150 ms), toggle thumb slide (120 ms), running-build ring pulse (2 s, disabled under reduced-motion). Nothing else.

## Avoid
Glossy skeuomorphism, wood/metal textures, drop shadows everywhere, dark mode, using the source colors for anything other than source identity.
