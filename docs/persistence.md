# Persistence & Snapshots

Kaiforth implements a versioned state-saving architecture to allow the VM to evolve without losing its learned optimization intelligence.

## 1. Versioned Container
Snapshots are wrapped in a `PersistenceContainer` which includes:
- **Magic Number**: `"KAIFORTH"` (8 bytes).
- **Version**: Current version is `6`.
- **Payload**: The serialized `OptimizerState`.

## 2. Integrity Verification
Before a snapshot is loaded:
1. **Magic Validation**: Rejects any file not starting with the Kaiforth magic.
2. **Version Compatibility**: Rejects snapshots from incompatible VM versions to prevent bytecode desync or ABI mismatches.
3. **Graceful Failure**: If validation fails, the VM boots "Fresh" instead of attempting to load potentially corrupted state.

## 3. Warm-Up Performance
By reloading `context_patterns` and `super_instructions`, a restarted VM avoids the "Cold Start" penalty and begins executing JIT-optimized blocks immediately upon seeing a known pattern.
