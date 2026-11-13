# Agent guide

## Project structure

- `src/` contains the vanilla HTML, CSS, and JavaScript frontend.
- `src-tauri/` contains the Tauri 2 and Rust backend.
- `src-tauri/tauri.conf.json` points `frontendDist` to `../src`.
- `workbook.schema.json` describes the saved workbook data.

## Project conventions

- Keep user-facing text in Armenian and follow the existing UI patterns.
- Keep changes focused; avoid adding frontend dependencies or build tooling unless requested.
- Workbook files use the `.pdw` extension and are ZIP archives containing `workbook.json` and image assets.
- Math Grid state stores its dimensions, cell contents, and border data.
- Keep `workbook.schema.json` and the user documentation in sync with changes to the saved format or user workflows.
- When changing the application version, update both `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.

## Validation

- Check frontend syntax with `node --check src/main.js` and `node --check src/components/math-grid/math-grid.js`.
- Check Rust formatting with `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`.
- Run backend tests with `cargo test --manifest-path src-tauri/Cargo.toml`.
