# Recording scenarios

Scripted browser recordings used to check the plans Macroscope recommends. Nothing here is real data.

    swift render.swift scenarios.json out            # renders the frames as PNGs
    META_API_KEY=... cargo run -p macroscope-app-core --example recording_scenarios -- \
      --confirm-spend scenarios.json out [name ...]   # real analysis + build + run

Each run prints the recommended plan (steps and their kind, what changes per run, what stays the same,
the questions) and then the built tool's live answer. Judge it against the `expect` line.
`SCENARIO_DEBUG=1` also prints the generated program.
