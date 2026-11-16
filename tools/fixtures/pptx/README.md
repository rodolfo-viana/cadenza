# Foreign presentation fixture

`foreign-libreoffice.pptx` was authored independently of the Cadenza model by converting the adjacent flat OpenDocument source with LibreOfficeDev 26.8.0.0.alpha0 (`2c87e51eeaa2b413ff4ae097b2705eea1995d8e5`) on 2026-11-15. It contains two slides, Latin/Portuguese/Greek text, a rectangle, an ellipse and an editable table. It has no private Cadenza part.

Recreate it from the repository root:

```sh
soffice -env:UserInstallation=file:///tmp/cadenza-pptx-fixture-profile \
  --headless --convert-to pptx --outdir tools/fixtures/pptx \
  tools/fixtures/pptx/foreign-libreoffice.fodp
```

The Rust importer tests read the committed PPTX; LibreOffice is not a runtime or test dependency. Additional tests construct minimal OOXML packages in memory to exercise relationship order, masters/layouts/themes, crop, missing chart values, groups, notes, links, unsupported objects, structural failures and bounded resources.
