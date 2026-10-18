<!-- MD041 - This file will be included in docs and should not start with a top header -->
<!-- Use 'cargo run -p build-templated-pages -- update features' to generate this file -->
<!-- markdownlint-disable-file MD041 -->

## Cargo Features

Hearth exposes many Cargo features to customize the engine. Enabling them adds functionality but may come at the cost of longer compilation times
and extra dependencies.

### Profiles

"Profiles" are high-level groups of cargo features that provide the full Hearth experience, but scoped to a specific domain.
These exist to be paired with `default-features = false`, enabling compiling only the subset of Hearth that you need.
This can cut down compile times and shrink your final binary size.

For example, you can compile only the "2D" Hearth features (without the 3D features) like this:

```toml
hearth = { version = "0.18", default-features = false, features = ["2d"] }
```

|Profile|Description|
|-|-|
{% for feature in features %}{% if feature.is_profile %}|{{ feature.name }}|{{ feature.description }}|
{% endif %}{% endfor %}
By default, the `hearth` crate enables the {% for feature in features %}{% if feature.is_default %}`{{ feature.name }}`{% endif %}{% endfor %} features.

### Collections

"Collections" are mid-level groups of cargo features. These are used to compose the high-level "profiles". If the default profiles don't
suit your use case (ex: you want to use a custom renderer, you want to build a "headless" app, you want to target `no_std`, etc), then you can use these
collections to build your own "profile" equivalent, without needing to manually manage _every single_ feature.

|Collection|Description|
|-|-|
{% for feature in features %}{% if feature.is_collection %}|{{ feature.name }}|{{ feature.description }}|
{% endif %}{% endfor %}
### Feature List

This is the complete `hearth` cargo feature list, without "profiles" or "collections" (sorted by name):

|Feature|Description|
|-|-|
{% for feature in sorted_features %}{% if feature.is_collection == False and feature.is_profile == False %}|{{ feature.name }}|{{ feature.description }}|
{% endif %}{% endfor %}