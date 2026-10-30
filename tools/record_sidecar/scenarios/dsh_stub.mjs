// Load npm/bladebro/dsh-plugin.mjs with a stub DSH bridge and print the options the
// plugin hands DSH's stdio MCP bridge (command, args, env, cwd, timeout), as JSON.
//   node dsh_stub.mjs REPO CONFIG_JSON OUT_JSON
import { cpSync, mkdirSync, writeFileSync, mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';
const [repo, configJson, outPath] = process.argv.slice(2);
const dir = mkdtempSync(join(tmpdir(), 'dshstub-'));
cpSync(join(repo, 'npm/bladebro/dsh-plugin.mjs'), join(dir, 'dsh-plugin.mjs'));
cpSync(join(repo, 'npm/bladebro/binary.cjs'), join(dir, 'binary.cjs'));
const mod = join(dir, 'node_modules/@deepseek-ai/dsh-mcp-client');
mkdirSync(mod, { recursive: true });
writeFileSync(join(mod, 'package.json'), JSON.stringify({ name: '@deepseek-ai/dsh-mcp-client', type: 'module', main: 'index.mjs' }));
writeFileSync(join(mod, 'index.mjs'), "export const name = 'dsh-mcp-client-stub';\n");
const plugin = await import(pathToFileURL(join(dir, 'dsh-plugin.mjs')).href);
let captured = null;
const ctx = { plugin: async (_bridge, opts) => { captured = opts; } };
await plugin.apply(ctx, JSON.parse(configJson));
console.log('PLUGIN', plugin.name, 'inject', JSON.stringify(plugin.inject));
console.log('BRIDGE OPTIONS', JSON.stringify(captured));
writeFileSync(outPath, JSON.stringify(captured, null, 2));
