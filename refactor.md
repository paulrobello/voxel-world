# Refactoring Workflow

This document outlines the workflow for decomposing large files into smaller, more manageable modules.

## General Process

### Planning Phase
1. Identify large files (>1000 lines) that need refactoring
2. Analyze the file structure and identify logical separations
3. Plan the module structure (what goes where)
4. Identify dependencies between components

### Execution Phase
1. Create directory structure: `mkdir -p src/module_name`
2. Create `mod.rs` with module declarations and re-exports
3. Extract code in logical order (dependencies first)
4. Update imports in extracted modules
5. Update imports in files that use the extracted code
6. Remove extracted code from original file

### Verification Phase
After each extraction:
1. Run `cargo check` - Verify compilation
2. Run `cargo clippy` - Check for warnings
3. Run `cargo test` - Ensure tests pass
4. Run `make checkall` - Full verification
5. Commit changes with descriptive message

## Best Practices

### Module Organization
- Keep modules focused (single responsibility)
- Target 50-600 lines per file
- Use meaningful module names
- Group related functionality
- Re-export at module level for clean API

### Import Strategy
- Use absolute paths from crate root for clarity
- Group imports: std, external crates, internal modules
- Update imports incrementally as code is extracted
- Use `pub(crate)` for cross-module access within same crate

### Git Workflow
- Use `git mv` when moving files to preserve history
- Commit after each major extraction
- Write descriptive commit messages following convention:
  - `refactor(module): decompose file into submodules`
- Keep commits atomic (one logical change per commit)

### Visibility Guidelines
- Start with private by default
- Use `pub` only for public API
- Use `pub(crate)` for internal cross-module access
- Keep implementation details private

## Common Patterns

### State Structures
Extract state structs before the code that uses them:
```
src/app_state/
├── mod.rs           # Re-exports
├── graphics.rs      # Graphics resources
├── simulation.rs    # Simulation state
└── ui_state.rs      # UI state
```

### Implementation Methods
Split large impl blocks by functionality:
```
src/app/
├── core.rs          # Struct definition + core methods
├── init.rs          # Initialization logic
├── update.rs        # Update loop
└── render.rs        # Rendering logic
```

### Helper Functions
Create helper modules for utility functions:
```
src/module/
├── mod.rs           # Main functionality
├── helpers.rs       # Utility functions
└── types.rs         # Type definitions
```

## Troubleshooting

### Import Errors
- Check module declarations in parent `mod.rs`
- Verify re-exports are public
- Use absolute paths for clarity

### Circular Dependencies
- Extract shared types to separate module
- Use trait definitions to break cycles
- Consider splitting into smaller modules

### Test Failures
- Update test imports
- Ensure test-only code is `#[cfg(test)]`
- Check that all functionality is still accessible

## Verification Checklist

- [ ] All files compile without errors
- [ ] No clippy warnings
- [ ] All tests pass (106/106)
- [ ] Module documentation added
- [ ] Imports organized and cleaned
- [ ] Visibility modifiers correct
- [ ] Git history preserved (used `git mv`)
- [ ] Commit messages descriptive
- [ ] Code runs correctly in-game
