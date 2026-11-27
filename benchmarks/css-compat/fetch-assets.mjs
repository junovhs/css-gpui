import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const [directory] = process.argv.slice(2);
assert(directory, 'Usage: node fetch-assets.mjs <asset cache directory>');
const { external } = JSON.parse(await readFile(new URL('corpus.json', import.meta.url)));
await mkdir(directory, { recursive: true });
for (const [url, { name, sha256, fetchUrl = url }] of Object.entries(external)) {
    const path = join(directory, name);
    const cached = await readFile(path).catch(error => {
        if (error.code !== 'ENOENT') throw error;
        return null;
    });
    if (cached && createHash('sha256').update(cached).digest('hex') === sha256) continue;
    const response = await fetch(fetchUrl, { signal: AbortSignal.timeout(30_000), headers: {
        'User-Agent': 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36',
    } });
    assert(response.ok, `${url}: HTTP ${response.status}`);
    const bytes = Buffer.from(await response.arrayBuffer());
    assert.equal(createHash('sha256').update(bytes).digest('hex'), sha256, `upstream bytes changed: ${url}`);
    await writeFile(path, bytes);
}
