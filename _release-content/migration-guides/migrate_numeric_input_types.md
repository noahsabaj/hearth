---
title: Migrate and rename numeric input types to hearth_ui_widgets
pull_requests: [25868]
---

Migrated `NumberFormat`, `NumberInputValue` and `NumberInputRange` from `hearth_feathers` to `hearth_ui_widgets`. As part of this, renamed them to `NumericFormat`, `NumericValue` and `NumericRange`.

Update the imports as follows:

```diff
use hearth::{
-   feathers::{NumberFormat, NumberInputValue, NumberInputRange},
+   ui_widgets::{NumericFormat, NumericValue, NumericRange},
};
```
