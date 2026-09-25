# wasm-demos

Four non-interactive Node CLI demos for `koru-lambda-core`, mirroring
the four Rust `examples/*.rs` bins. Each demo runs, prints results to
stdout, and exits — no browser, no clicks.

## One-time setup

Build a local Node-target wasm package (the npm-shipped `pkg/` uses
`--target bundler` which is browser/bundler-shaped; Node CLI needs
`--target nodejs`):

```bash
# from the repo root
npm --prefix examples/wasm-demos run build:pkg

# then install demo deps
cd examples/wasm-demos
npm install
```

`build:pkg` calls `wasm-pack build --target nodejs` on the bindings crate with `--out-dir
examples/wasm-demos/pkg-node ../..` and writes into
`examples/wasm-demos/pkg-node/` (gitignored). `package.json`'s
`"koru-lambda-core": "file:./pkg-node"` resolves to that build.

## Run any demo

```bash
npm run demo:1   # hello_synth
npm run demo:2   # novelty_and_chain
npm run demo:3   # projection_composition
npm run demo:4   # wire_format_roundtrip
npm run demo:all # runs all four, back-to-back
```

Each demo prints exactly the same use-case as its Rust twin under
`examples/NN_*.rs`; the two outputs should read identically modulo
formatting.

## Typecheck

```bash
npm run typecheck
```

Runs `tsc --noEmit` against the strict `tsconfig.json`.

## After the npm publish

Once `koru-lambda-core@2.0.0` is on npmjs.com, the `--target nodejs`
build isn't strictly required for CLI demos — Node ≥ 22 supports
importing the bundler-target package via ESM with
`--experimental-wasm-modules`, but the flag is not stable. The
`pkg-node/` path is the ergonomic default.
