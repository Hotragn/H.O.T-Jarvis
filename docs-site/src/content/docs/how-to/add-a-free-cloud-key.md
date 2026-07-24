---
title: Add a free cloud key
description: Use a free cloud tier (Groq or OpenRouter) instead of, or as a fallback to, a local model.
---

import { Tabs, TabItem } from '@astrojs/starlight/components';

H.O.T-Jarvis prefers a local model, but you can add a free cloud tier — as your
only provider (no Ollama needed) or as a fallback when the local model is down.
No paid plan is required.

## 1. Get a free key

<Tabs>
  <TabItem label="Groq">
    Create a key at [console.groq.com/keys](https://console.groq.com/keys). Fast,
    generous free tier.
  </TabItem>
  <TabItem label="OpenRouter">
    Create a key at [openrouter.ai/keys](https://openrouter.ai/keys) and use a
    model whose id ends in `:free`.
  </TabItem>
</Tabs>

## 2. Put it in `.env`

Copy the example file and add your key:

```bash frame="terminal"
cp .env.example .env
```

```ini title=".env"
# one is enough — the router uses whatever is present
GROQ_API_KEY=gsk_your_key_here
# GROQ_MODEL=llama-3.3-70b-versatile

# OPENROUTER_API_KEY=sk-or-your_key_here
# OPENROUTER_MODEL=meta-llama/llama-3.3-70b-instruct:free
```

## 3. Restart

Restart the app. The router tries **Ollama first** (local, private), then any
configured cloud provider in order. The reply footer shows which one answered.

## How routing behaves

- **Local is always preferred.** Cloud is only used when Ollama isn't reachable.
- **Free-tier hygiene is built in.** Identical requests are served from a short
  cache, and a provider that rate-limits is skipped on a cooldown instead of
  hammered. Full detail in [Configuration](/reference/configuration/).
- **The local model is never penalized** — it's unlimited, so it stays first.
