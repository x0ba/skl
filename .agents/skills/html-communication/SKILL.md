---
name: html-communication
description: When the user asks for an HTML write-up of work (NOT as part of the codebase)
---

# HTML Communication

## When to use

Use this skill for any request to produce a readable HTML artifact for a human - whether it is called a plan, a spec, a write-up, findings, a summary, a report, a comparison, or a set of UI mocks. The word "plan" is often absent. What the requests share is: a document to read outside the terminal, and a link to open it.

Also use it whenever a user supplies an HTML plan URL to read.

Do **not** use it for HTML that is part of the product being built (app templates, components, marketing pages). This skill is for documents about work, not shipped UI.

## Document

Create one self-contained HTML file, capped at 512 KB.

- Write it like a spec, not a landing page: dense, scannable, no hero, decorative chrome, marketing voice, or em dashes. 
- Default to true black ('#000'), white primary text, and dark gray only for secondary surfaces or accents. 
- Make it mobile-readable with a responsive viewport and no fixed-width layout. 
- Use semantic HTML, inline CSS, inline SVG, and HTTPS or data-URL images. 
- Use an inline classic script only when interactivity materially helps. Keep scripted pages useful without JavaScript.
- In script-free files, give external links 'target="_blank"' and 'rel="noopener noreferrer"'. If any script exists, omit 'target="_blank"'.

Never include external or module scripts, inline event handlers, 'javascript:' URLs, forms, frames, embeds, objects, applets, meta refresh, linked stylesheets, secrets, private URLs, or local filesystem paths.

## UI Mocks

When the user asks for variants:

- Render real styled variants, not descriptions. 
- Label them 'A' , 'B' ', 'C' ... for easy selection. 
- Lay them out for direct comparison. 
- Keep one file across iterations so its URL stays stable.
