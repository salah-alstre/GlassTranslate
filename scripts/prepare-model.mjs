// Rebuilds a bundled translation model from its public Helsinki-NLP/opus-mt-* checkpoint:
//   en-ar (default) -> src-tauri/resources/model        (scripts/model.sha256)
//   he-ar           -> src-tauri/resources/model-he-ar  (scripts/model-he-ar.sha256)
//
// The CTranslate2 conversion is not hosted anywhere, so it is reproduced locally:
//   1. a Python venv with ctranslate2 + transformers + sentencepiece + torch is created,
//   2. ct2-transformers-converter converts the checkpoint (int8),
//   3. the sentencepiece files are copied next to it,
//   4. every produced file is checked against scripts/model.sha256 (pass --update to rewrite it).
//
// Usage: node scripts/prepare-model.mjs [en-ar|he-ar] [--update]
// Needs: Python 3.9+ on PATH and ~3 GB of temporary disk space. Use a SHORT venv path on Windows (MAX_PATH).
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { tmpdir } from 'node:os'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const MODELS = {
  'en-ar': { hf: 'Helsinki-NLP/opus-mt-en-ar', dir: 'model', sums: 'model.sha256' },
  'he-ar': { hf: 'Helsinki-NLP/opus-mt-he-ar', dir: 'model-he-ar', sums: 'model-he-ar.sha256' },
}
const which = process.argv.slice(2).find((a) => !a.startsWith('--')) ?? 'en-ar'
const spec = MODELS[which]
if (!spec) {
  console.error(`Unknown model "${which}". Choose one of: ${Object.keys(MODELS).join(', ')}`)
  process.exit(1)
}
const outDir = join(root, 'src-tauri', 'resources', spec.dir)
const sumsFile = join(root, 'scripts', spec.sums)
const update = process.argv.includes('--update')
const FILES = ['model.bin', 'config.json', 'shared_vocabulary.json', 'source.spm', 'target.spm']

const sha = (p) => createHash('sha256').update(readFileSync(p)).digest('hex')
const run = (cmd, args, opts = {}) => execFileSync(cmd, args, { stdio: 'inherit', ...opts })

function verify() {
  if (!existsSync(sumsFile)) return false
  const want = Object.fromEntries(
    readFileSync(sumsFile, 'utf8')
      .split(/\r?\n/)
      .filter(Boolean)
      .map((l) => l.split(/\s+/).reverse()),
  )
  return FILES.every((f) => existsSync(join(outDir, f)) && sha(join(outDir, f)) === want[f])
}

if (!update && verify()) {
  console.log('Model already present and checksums match.')
  process.exit(0)
}

const work = join(process.platform === 'win32' ? 'C:\\' : tmpdir(), `gtconv-build-${which}`)
rmSync(work, { recursive: true, force: true })
mkdirSync(work, { recursive: true })
const py = process.platform === 'win32' ? 'python' : 'python3'
const venv = join(work, 'v')
run(py, ['-m', 'venv', venv])
const vpy = join(venv, process.platform === 'win32' ? 'Scripts' : 'bin', process.platform === 'win32' ? 'python.exe' : 'python')
run(vpy, ['-m', 'pip', 'install', '--quiet', 'ctranslate2', 'transformers', 'sentencepiece', 'torch', 'huggingface_hub'])

const ckpt = join(work, 'ckpt')
run(vpy, ['-c', `from huggingface_hub import snapshot_download as s; s('${spec.hf}', local_dir=r'${ckpt}')`])
const conv = join(work, 'ct2')
const convBin = join(venv, process.platform === 'win32' ? 'Scripts' : 'bin', 'ct2-transformers-converter')
run(convBin, ['--model', ckpt, '--output_dir', conv, '--quantization', 'int8', '--force'])

mkdirSync(outDir, { recursive: true })
for (const f of ['model.bin', 'config.json', 'shared_vocabulary.json']) cpSync(join(conv, f), join(outDir, f))
for (const f of ['source.spm', 'target.spm']) cpSync(existsSync(join(conv, f)) ? join(conv, f) : join(ckpt, f), join(outDir, f))

const sums = FILES.map((f) => `${sha(join(outDir, f))}  ${f}`).join('\n') + '\n'
if (update || !existsSync(sumsFile)) {
  writeFileSync(sumsFile, sums)
  console.log('Wrote', sumsFile)
} else if (!verify()) {
  console.warn(`WARNING: converted files differ from ${spec.sums} (a newer converter can change model.bin bytes).`)
  console.warn('Review the output and run with --update if the translation test still passes.')
}
console.log('Model ready in', outDir, readdirSync(outDir).join(', '))
