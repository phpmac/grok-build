---
name: block-github-via-scraper
enabled: true
event: mcp
action: block
conditions:
  - field: tool_name
    operator: regex_match
    pattern: mcp__plugin_a_(firecrawl|brave-search|exa|jina)__
  - field: _all_text
    operator: regex_match
    pattern: github\.com|githubusercontent\.com
---

访问github必须使用gh命令