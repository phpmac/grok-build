---
name: block-xcom-via-scraper
enabled: true
event: mcp
action: block
conditions:
  - field: tool_name
    operator: regex_match
    pattern: mcp__plugin_a_(firecrawl|brave-search|exa)__
  - field: _all_text
    operator: regex_match
    pattern: (?:x|twitter)\.com/|t\.co/
---

禁用通用爬虫处理 X/Twitter 数据, 改用专用工具 mcp__plugin_x_grok__chat:

- 读推文/搜帖/用户与评价/网页 -> chat(prompt) (自然语言, 可贴 URL)

通用爬虫爬 x.com 会被反爬, 拿不到完整线程和引用; grok chat 直连 xAI 返回结构化结果.
