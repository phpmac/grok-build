---
name: block-curl-github
enabled: true
event: bash
action: block
pattern: (?:^|[|;&()]|\$\(\s*|xargs\s+)\s*curl\b[^|;&\n]{0,300}?(?:github\.com|githubusercontent\.com)
---

禁止使用 curl 访问github,按照规范使用gh