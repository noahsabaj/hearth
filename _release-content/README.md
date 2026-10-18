# Release Content

This directory contains drafts of documentation for the current development cycle, which will be published with the next release. You can find more information in the [release notes](./release_notes.md) and [migration guides](./migration_guides.md) files.

## Publishing

When it is time to prepare the final release notes and migration guide, use the following process:

1. Check out the upcoming Hearth release branch in the `hearth` repo
2. Open a terminal, navigate to the `tools/export-content` folder in the `hearth` repo, and run `cargo run`.
3. Use the tool to easily reorder the release notes relative to each other
4. After saving and exiting the tool, it will write `merged_release_notes.md` and `merged_migration_guides.md` to `_release-content`
5. Edit these into the release's notes and migration guide.
