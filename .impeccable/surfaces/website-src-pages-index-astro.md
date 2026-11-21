---
version: 1
slug: "website-src-pages-index-astro"
primary_target: "website/src/pages/index.astro"
related_targets: ["website/src/pages/en/index.astro"]
---

## Surface

- Target: `website/src/pages/index.astro` (vi default) + paired English home route.
- Mode: Persuade. Audience: new multi-account Zalo desktop users, especially operators needing separate work/personal/support profiles; also first-time/basic users.
- Primary outcome: visitor understands NexaDock by exploring one realistic workflow, then chooses the correct OS install route. Proof: local sample workspace with explicit “no live Zalo connection” label, real product screenshot and verified isolation facts.
- Selected structure: **Workflow Library** (`workflow-gallery.html`, picked by user from the surface concept round; `seed:2fec3f840f658eb5`). A task index on the left; one selected workflow on the right with a short guided path and a real next action.
- Prior user choices preserved: interactive workflow first; hybrid exploration catalog + guided installation. Keep Rack Console visual world, token contract in DESIGN.md, no VNG binaries, Experimental stays Experimental, no invented claims/testimonials. Vi/en parity.
- Scope: homepage first-view and its below-fold path may be re-sequenced to deliver the selected structure; retain current docs/install guide routes and their behavior. Nav Releases label stays versionless; beta.9 version only in footer/download.
- Code-led build (no image generation). Responsive: desktop split index/detail; phone becomes horizontally browseable task rail followed by selected workflow. States: selected, unselected, OS Experimental, sample-data disclosure. Anti-goal: no generic hero+feature-card landing, no equal card grid, no live-data implication.

### Direction contract

THESIS: Start with a task library, not a brochure: one selected multi-Zalo workflow is visible and actionable above the fold.

OWN-WORLD: Rack Console stays the material—near-black ground, hairline panels, restrained green/amber/gray LEDs; mono only for measured task IDs and status. No category-standard SaaS card wall.

STORY: Choose a real need (run a separate profile, verify isolation, pick the Linux route), see the relevant state and caveat, then follow a short guide to the next action.

FIRST VIEWPORT: At desktop, a narrow numbered workflow rail sits left and one expanded workflow panel right; the panel shows steps, an isolation proof line and one CTA. At mobile, the rail becomes a horizontal scroll strip above the expanded task. Label all data as sample; do not imply the demo is connected to Zalo.

FORM: A browseable workflow index + one selected-detail surface with guided steps, not a static section stack. Selection must update the detail deterministically; keyboard and touch work, focus is visible, reduced motion is honored.

DEPTH: Below the fold, keep the same selected workflow grammar for proof and platform routes; provide separate guided install paths for Windows/macOS/Linux, with Linux clearly Experimental and voice/sync limitations adjacent to its action. Build path: code-led; seed:2fec3f840f658eb5.
