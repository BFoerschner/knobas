# D5 — Index card

**Identity:** a desk with index cards. Light, tactile, friendly. Remote entities are neat printed cards; your own notes are hand-filed index cards pinned on top of them. Rounded, soft shadows, warm but not cream, with plum and moss as the two accents. The most approachable of the five — it should feel like you could hand it to a PM.

## Tokens
- Desk `#EEF2EC` sage-gray · Card `#FFFFFF` · Card edge `#D8E0D5` · Shadow `rgba(39,49,43,.10)`
- Text `#27312B` · Muted `#6F7A72` · Faint `#A3ACA6`
- **Plum `#6B3F8A`** — primary action, selection, links
- **Moss `#4F7A3E`** — success, confirmed, done, timer running
- Attention `#D9822B` amber-orange (inbox, failed builds use `#C4453A`)
- **Note card `#FFF6C2`** pale yellow (sticky/index card tint) with a lined-paper pattern (1 px `rgba(0,0,0,.06)` every 24 px)
- Source identity: a small rounded tab on the top-left of each card with the source name (Jira / Wiki / Git / CI), tinted (`#E4E9FF`, `#EEE4FF`, `#E1F2E4`, `#FFE9D6`) with dark text — gentle, not saturated.

## Type (Google Fonts)
- Display / headings: **Bricolage Grotesque** 600–700 (optical size on, wide-ish)
- Body & UI: **Figtree** 400/500/600
- Keys, times, hashes: **JetBrains Mono** 400

## Shape & density
- 12–14 px radii on cards, 8 px on controls, 999 px on chips. Soft two-layer shadow (`0 1px 2px`, `0 6px 16px` of the shadow token). Comfortable density: rows 40–44 px, cards with 16–20 px padding. Hover lifts a card by 1 px + shadow.

## Signature
**Pinned index cards.** Local notes render as yellow lined index cards, rotated −1.5°…+1.5° (seeded per note), with a pin (inline SVG: a round head + a tiny shadow) or a strip of translucent tape at the top edge. When a note is linked to an entity, the note card sits *pinned on the corner of* the entity's card (overlapping by ~12 px) — e.g. *SEPA retry investigation* pinned on PAY-231. Dragging a note card onto an entity card links them (one drag-to-pin must work). The worklog draft is a card too: a small "time card" with a punched-clock look (ticked rows), and *Log 2h 44m* "stamps" it moss-green.

## Motion
Card lift on hover (120 ms), pin drop (200 ms scale+rotate settle), toast slide. Reduced-motion: none of the rotations animate (static rotation stays).

## Avoid
Candy gradients, pastel-everything SaaS, terracotta/cream, emoji as icons, oversized hero headings, cards inside cards inside cards — only notes are "objects on top"; everything else is flat cards on the desk.
