fn main() {
    // WASM build is optional in this environment; production builds should enable `substrate-wasm-builder`.
    #[cfg(all(feature = "std", feature = "wasm-builder"))]
    {
        substrate_wasm_builder::WasmBuilder::new()
            .with_current_project()
            .export_heap_base()
            .import_memory()
            .build();
    }
}
