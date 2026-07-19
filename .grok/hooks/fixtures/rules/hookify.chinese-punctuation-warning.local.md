---
name: warn-chinese-punctuation
enabled: true
event: file
action: warn
hook_events:
  - PreToolUse
conditions:
  - field: content
    operator: regex_match
    pattern: [，。！？；：“”‘’【】《》、\U0001F000-\U0001FAFF\U00002300-\U000023FF\U00002600-\U000027BF\U00002B00-\U00002BFF\U0000FE00-\U0000FE0F]
---

**避免使用中文标点符号或 emoji 表情**
