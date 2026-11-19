# Provider marks

AEROS shows these marks only to identify the matching integration in Agent Hub, the chat and the Companion. They are never combined with the AEROS identity, recoloured beyond the published brand colour, or shown larger than AEROS. All marks remain the property of their owners.

| File | Mark | Source | Notes |
| --- | --- | --- | --- |
| `claude.svg` / `claude.png` | Claude | [Simple Icons 16.38.0](https://cdn.jsdelivr.net/npm/simple-icons@16.38.0/icons/claude.svg), brand colour `#D97757` | Simple Icons collects official vector marks under CC0; trademark rights stay with Anthropic. |
| `claudecode.svg` / `claudecode.png` | Claude Code | [Simple Icons 16.38.0](https://cdn.jsdelivr.net/npm/simple-icons@16.38.0/icons/claudecode.svg) | Embedded for future use. |
| `anthropic.svg` / `anthropic.png` | Anthropic | [Simple Icons 16.38.0](https://cdn.jsdelivr.net/npm/simple-icons@16.38.0/icons/anthropic.svg) | Not displayed. |
| `googlegemini.svg` / `googlegemini.png` | Google Gemini | [Simple Icons 16.38.0](https://cdn.jsdelivr.net/npm/simple-icons@16.38.0/icons/googlegemini.svg), brand colour `#8E75B2` | Google brand guidance: [Brand Resource Center](https://about.google/brand-resource-center/). |
| `antigravity.svg` / `antigravity.png` | Google Antigravity | `resources/app/out/media/jetski-logo-white.svg` from the locally installed Antigravity IDE | Official monochrome mark shipped with the product. Press assets: [antigravity.google/press](https://antigravity.google/press). |

PNG files are 128 px white RGBA masks rasterised with resvg from the SVG next to them. The editor tints them at draw time.

## OpenAI, ChatGPT and Codex

OpenAI's design guidelines at [openai.com/brand](https://openai.com/brand/) allow the Blossom only where it relates directly to OpenAI services, without added colour, not more prominent than your own brand, and after accepting OpenAI's terms of use. The logo download is gated behind that terms checkbox. OpenAI also asked Simple Icons to remove its mark. Accepting those terms is the owner's decision, so this build uses a neutral terminal monogram for Codex. To switch, download the official package after accepting the terms, export the monochrome Blossom as a 128 px white mask to `openai.png`, register it in `src/editor/assets.rs` and set `mark: Some("brand_openai")` for `Provider::Codex` in `src/editor/brands.rs`.
