---
title: Entity inspection tools
authors: ["@jbuehler23", "@alice-i-cecile", "@Nilirad", "@Zeophlite"]
pull_requests: [25818, 25822, 25823, 25824, 25826, 25837, 25845, 25866, 25883, 25884, 25885, 25923, 26014, 26063, 26172, 26171, 26163]
---

`hearth_dev_tools::inspection` is gaining a backend for inspecting worlds, entities, components, and resources.

This note will be completed once the rest of the series lands.

- Added `app.info` to the Hearth Remote Protocol (#25822)
- Added `diagnostics.list` and `diagnostics.get` to the Hearth Remote Protocol (#25824)
- Added `WorldSummary` to `hearth_dev_tools` inspection (#25818)
- Added component inspection to `hearth_dev_tools` (#25823)
- Added `stepping.*` methods to the Hearth Remote Protocol (#25826)
- Added a native HRP client behind the `client` feature (#25837)
- Added entity and resource inspection to `hearth_dev_tools` (#25845)
- Added serde support and structured values to the inspection types (#25866)
- Added `world.inspect*`, `world.summarize` and `registry.component_metadata` to the Hearth Remote Protocol (#25883)
- Added the `hearth_inspector` crate with a local entity tree and an editable details panel (#25884, #25885, #25923)
- Added a remote source to `hearth_inspector`, showing the entity tree of a separate running app over the Hearth Remote Protocol (#26014)
- Added color picking and enum variant switching to the `hearth_inspector` details panel (#26063)
- Added `type_path` to `ComponentInspection` and `ComponentTypeMetadata`, so remote inspectors can match results to schema types
- `registry.schema` now includes each type's default value when it registers `ReflectDefault`
- Added a connection panel to the `hearth_inspector` remote source
