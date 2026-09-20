# Independent C# fixtures

Generated from `suilevap/PavEcsGame` at
`bc0b449f68d953861932214b85110f99925d886a` by:

```sh
python3 tools/generate_reference.py /path/to/PavEcsGame
```

Requires .NET 10 only when regenerating; Rust tests read the checked-in TSVs.

- `fov_reference.tsv`: 5,769 ordered samples in 9 cases. Map cases use the
  first player (or map center when absent), radius 16, and the original
  out-of-bounds/static-wall obstacle predicate. Tiny C# roundoff is retained.
- `light_reference.tsv`: 2,880 exact light-merge outputs.
- `palette_reference.tsv`: all 1,024 kind/intensity-to-console-color outputs.

The generator compiles upstream algorithms, extracts the lighting routines
from ECS wrappers, and omits the unrelated broken `Float3` type. It does not
translate those algorithms to Python. See `REFERENCE_COMPARISON.md` for scope.
