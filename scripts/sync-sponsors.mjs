#!/usr/bin/env node

/**
 * sync-sponsors.mjs
 *
 * Reads sponsors.json and regenerates SPONSORS.md automatically.
 * Can be run manually or triggered by a CI webhook.
 *
 * Usage:
 *   node scripts/sync-sponsors.mjs
 *
 * To integrate with Ko-fi webhook:
 *   1. Set up a GitHub Action that listens for `repository_dispatch` events
 *   2. Ko-fi webhook → your endpoint → fires `repository_dispatch` with sponsor data
 *   3. GitHub Action runs this script and commits the updated SPONSORS.md
 */

import { readFileSync, writeFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..');

const sponsorsPath = join(ROOT, 'sponsors.json');
const output = join(ROOT, 'SPONSORS.md');

let data;
try {
  data = JSON.parse(readFileSync(sponsorsPath, 'utf-8'));
} catch (err) {
  console.error('❌ Could not read sponsors.json:', err.message);
  process.exit(1);
}

const { sponsors = [] } = data;

const tiers = {
  founding:  { label: '💎 Founding Supporters (€15+)', items: [] },
  monthly:   { label: '🔄 Monthly Backers', items: [] },
  champion:  { label: '☕☕ Champions (€5-14)', items: [] },
  supporter: { label: '☕ Supporters (€1-4)', items: [] },
};

// Sort sponsors into tiers (skip 'founder' seed entry)
for (const s of sponsors) {
  if (s.tier === 'founder') continue;
  if (tiers[s.tier]) {
    tiers[s.tier].items.push(s);
  }
}

const today = new Date().toISOString().split('T')[0];

let md = `# FlowSight Sponsors

Thank you to everyone who supports FlowSight development! 💜

> To be listed here, support the project on [Ko-fi](https://ko-fi.com/flowsight).

---

`;

for (const [, tier] of Object.entries(tiers)) {
  md += `## ${tier.label}\n\n`;
  if (tier.items.length === 0) {
    md += `*Be the first! Your name here.*\n\n`;
  } else {
    // Sort by amount descending, then by date ascending
    const sorted = tier.items.sort((a, b) => {
      if (b.amount !== a.amount) return b.amount - a.amount;
      return new Date(a.timestamp) - new Date(b.timestamp);
    });
    for (const s of sorted) {
      const date = new Date(s.timestamp).toLocaleDateString('en-US', {
        month: 'short',
        year: 'numeric',
      });
      const link = s.url ? `[${s.name}](${s.url})` : `**${s.name}**`;
      const msg = s.message ? ` — _${s.message}_` : '';
      md += `- ${link} (${date})${msg}\n`;
    }
    md += '\n';
  }
  md += '---\n\n';
}

md += `## 🏢 Corporate Sponsors

Interested in corporate sponsorship? Email **manuel@flowsight.site**

---

*This file is updated automatically from \`sponsors.json\`.*
*Last updated: ${today}*
`;

writeFileSync(output, md, 'utf-8');
console.log(`✅ SPONSORS.md updated with ${sponsors.length - 1} sponsor(s).`);
