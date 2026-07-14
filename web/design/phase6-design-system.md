# NarraState Phase 6 UI system

Reference: `phase6-investigation-concept.png` (1536 × 1024).

## Product direction

The stable NarraState shell is genre-neutral. Cases may replace the accent color,
cover media, and content, but they do not replace the app layout or control
language. The shell must not rely on detective-specific dossier, police, noir,
paper, or evidence-board decoration.

## Tokens

- Shell: `#0f1419`; rail: `#12181e`; raised: `#182027`.
- Primary text: `#f4f5f6`; secondary: `#a7afb7`; divider: `#313b44`.
- Case accent: `#ef5b3f`; strong accent: `#ff6b4b`.
- Success: `#79b99a`; warning: `#e1ad63`; danger: `#dc6d6d`.
- Radius: 0 for regions, 4px for rows/inputs, 6px for primary controls.
- Spacing: 4, 8, 12, 16, 24, 32, 40px.
- Chrome type: Microsoft YaHei, PingFang SC, system sans-serif.
- Wordmark: Inter/Segoe UI/system sans-serif, 600, tracked.
- Motion: 140ms control feedback, 220ms panel movement; reduced-motion safe.

## Container model

- 64px global header.
- Desktop investigation: 25% / 42% / 33% open columns with 1px dividers.
- No nested card grid. Use rails, rows, ruled lists, tabs, and one composer frame.
- Mobile: one active workspace with bottom tabs for people, dialogue, clues, notes.

## Component families

- App header, wordmark, saved state, settings and primary conclusion action.
- Case row/cover, session row, configuration drawer.
- Person row, event row, transcript turn, streaming turn.
- Research tabs, clue row, statement row, inference note.
- Attachment row, question composer, SSE/recovery notice.
- Accusation dialog, conclusion report, developer drawer.
- 1.5px outline icons using `currentColor`, 20px default optical size.

## Player-facing copy guard

Normal mode may show case, people, event, clue, statement, inference, saved,
degraded, and recovery language. It must not show phase, stress, composure,
defense budget, prompt, token, LLM, disclosure IDs, or hidden facts.

## Required states

- Home: configured/unconfigured provider, Mock start, recent-session recovery.
- Brief: public summary, people, public timeline, initial clues, start.
- Investigation: empty, streaming, degraded, recovered, active attachments.
- Accusation: wrong target, insufficient evidence, proven without/with confession.
- Conclusion: result, truth timeline, decisive evidence, reasoning, turn count.
- Developer mode: explicit spoiler gate, then internal trace panel.
