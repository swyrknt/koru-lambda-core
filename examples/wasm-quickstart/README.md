# wasm-quickstart

A minimal Vite + TypeScript app that boots `koru-lambda-core` in the
browser and exercises every major method: primordials, synthesis, the
Axiom-4 trust boundary, and an `Adjacency` projection.

## Run it

```bash
npm install
npm run dev
# then open http://localhost:5173
```

The page renders a JSON summary of the operations to a `<pre>` block.

## Dependency resolution

`package.json` pins `"koru-lambda-core": "^2.0.0"`. Two modes:

- **After npm publish (S05 ships):** the `^2.0.0` spec resolves against
  npmjs.com — no change needed.
- **Before publish, for local testing:** rebuild `pkg/` at the repo root
  with `wasm-pack build --release --target bundler`, then edit
  `package.json` to override the dep with
  `"koru-lambda-core": "file:../../pkg"` and re-run `npm install`.

## Typecheck

```bash
npm run typecheck
```

Runs `tsc --noEmit` against the strict `tsconfig.json`.
