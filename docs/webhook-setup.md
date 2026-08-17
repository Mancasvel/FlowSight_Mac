# Ko-fi Webhook Setup

This guide explains how to automatically update the sponsors list when someone donates on Ko-fi.

## Architecture

```
User donates on Ko-fi
       ↓
Ko-fi sends webhook POST
       ↓
Cloudflare Worker (proxy)
       ↓
GitHub repository_dispatch API
       ↓
GitHub Action runs sync-sponsors.mjs
       ↓
SPONSORS.md updated + committed
```

## Step 1: Create a Cloudflare Worker (free)

Ko-fi can't send GitHub `repository_dispatch` events directly, so we use a lightweight proxy.

Create a new Cloudflare Worker with this code:

```javascript
export default {
  async fetch(request, env) {
    if (request.method !== 'POST') {
      return new Response('Method not allowed', { status: 405 });
    }

    // Parse Ko-fi webhook
    const formData = await request.formData();
    const raw = formData.get('data');
    if (!raw) return new Response('No data', { status: 400 });

    const data = JSON.parse(raw);

    // Validate token (optional but recommended)
    if (env.KOFI_TOKEN && data.verification_token !== env.KOFI_TOKEN) {
      return new Response('Unauthorized', { status: 401 });
    }

    // Only process donations
    if (data.type !== 'Donation' && data.type !== 'Subscription') {
      return new Response('Ignored', { status: 200 });
    }

    // Fire GitHub repository_dispatch
    const ghResponse = await fetch(
      `https://api.github.com/repos/Mancasvel/FlowSight.AI/dispatches`,
      {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${env.GITHUB_TOKEN}`,
          'Accept': 'application/vnd.github.v3+json',
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          event_type: 'kofi-donation',
          client_payload: {
            from_name: data.from_name,
            message: data.message,
            amount: data.amount,
            type: data.type,
            is_subscription: data.is_subscription,
            kofi_transaction_id: data.kofi_transaction_id,
          },
        }),
      }
    );

    if (!ghResponse.ok) {
      return new Response(`GitHub error: ${ghResponse.status}`, { status: 502 });
    }

    return new Response('OK', { status: 200 });
  },
};
```

## Step 2: Set environment variables

In your Cloudflare Worker settings, add:

| Variable | Value |
|----------|-------|
| `KOFI_TOKEN` | Your Ko-fi webhook verification token |
| `GITHUB_TOKEN` | A GitHub Personal Access Token with `repo` scope |

## Step 3: Configure Ko-fi webhook

1. Go to [ko-fi.com/manage/webhooks](https://ko-fi.com/manage/webhooks)
2. Set **Webhook URL** to your Cloudflare Worker URL, e.g.:
   `https://kofi-proxy.your-name.workers.dev`
3. Copy the **Verification Token** and set it as `KOFI_TOKEN` in your Worker

## Step 4: Test

Simulate a Ko-fi webhook:

```bash
curl -X POST https://kofi-proxy.your-name.workers.dev \
  -H "Content-Type: application/x-www-form-urlencoded" \
  --data-urlencode 'data={"type":"Donation","from_name":"Test User","message":"Testing!","amount":"5.00","kofi_transaction_id":"test-001","verification_token":"YOUR_TOKEN"}'
```

Then check the [Actions tab](https://github.com/Mancasvel/FlowSight.AI/actions/workflows/sync-sponsors.yml) to see if the workflow ran.

## Manual trigger

You can also trigger the sync manually:

1. Go to [Actions → Sync Sponsors](https://github.com/Mancasvel/FlowSight.AI/actions/workflows/sync-sponsors.yml)
2. Click "Run workflow"
3. Edit `sponsors.json` directly if needed, then re-run

## Troubleshooting

- **No workflow triggered**: Check that the Worker's `GITHUB_TOKEN` has `repo` scope
- **Duplicate sponsors**: The script deduplicates by `kofi_transaction_id`, so this shouldn't happen
- **Wrong tier**: Tiers are resolved by amount: €1-4 = Supporter, €5-14 = Champion, €15+ = Founding, Subscription = Monthly
