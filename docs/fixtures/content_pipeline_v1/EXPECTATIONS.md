# Content Pipeline v1 Fixture Expectations

This file defines expected outcomes for fixture directories under this folder.

General assumptions:

1. Base content is loaded first (`mod_id = "base"`).
2. Additional mods are loaded in the order listed per scenario.
3. `mod_id` is derived from the folder name.
4. `compiler_version` and `game_version` invalidation use exact string match.

## 1. Pass Cases

### pass_01_base_only

Load order:

1. `base`

Expected result: PASS.

Expected merged `EntityDef` values:

1. `defName = "proto.player"`
2. `label = "Player"`
3. `renderable = "Placeholder"`
4. `moveSpeed = 5.0`
5. `tags = ["colonist", "starter"]`

### pass_02_scalar_override

Load order:

1. `base`
2. `betterlabels`

Expected result: PASS.

Expected merged `EntityDef` for `proto.worker`:

1. `label = "Skilled Worker"` from `betterlabels` (scalar override).
2. `moveSpeed = 4.5` from `betterlabels` (scalar override).
3. `renderable = "Placeholder"` remains valid.
4. `tags = ["human", "labor"]` inherited from `base` because list not provided by later mod.

### pass_03_list_replace

Load order:

1. `base`
2. `replacetags`

Expected result: PASS.

Expected merged `EntityDef` for `proto.miner`:

1. `label = "Miner"`
2. `moveSpeed = 3.8`
3. `tags = ["specialist", "night_shift"]` from `replacetags`.
4. Base tags are replaced, not appended.

## 2. Fail Cases

### fail_01_missing_defname

Load order:

1. `missingdefname`

Expected result: FAIL.

Expected error properties:

1. `code` indicates missing required field.
2. `mod_id = "missingdefname"`.
3. `file_path` includes `missingdefname/defs.xml`.
4. best-effort line and column for missing `defName`.

### fail_02_unknown_field

Load order:

1. `unknownfield`

Expected result: FAIL.

Expected error properties:

1. `code` indicates unknown field/element.
2. `mod_id = "unknownfield"`.
3. `file_path` includes `unknownfield/defs.xml`.
4. field reference identifies `mood`.

### fail_03_invalid_enum

Load order:

1. `invalidenum`

Expected result: FAIL.

Expected error properties:

1. `code` indicates invalid enum value.
2. `mod_id = "invalidenum"`.
3. `file_path` includes `invalidenum/defs.xml`.
4. invalid value reported as `Sprite`.

### fail_04_duplicate_key_same_mod

Load order:

1. `dupkeys`

Expected result: FAIL.

Expected error properties:

1. `code` indicates duplicate `(def_type, def_name)` in a single mod.
2. `mod_id = "dupkeys"`.
3. `def_type = "EntityDef"`.
4. `def_name = "proto.dup"`.

### fail_05_type_mismatch

Load order:

1. `base`
2. `retype`

Expected result: FAIL.

Expected error properties:

1. `code` indicates cross-mod key collision with different `def_type`.
2. `def_name = "proto.shared_key"`.
3. conflicting types are `EntityDef` vs `ItemDef`.
4. `mod_id` and `file_path` identify both participating definitions.

## 3. Invalidation Checks (metadata-focused)

These checks are not separate XML fixtures; they are required behavior checks over any valid fixture set.

1. Change only `compiler_version` string:
   1. Example: `compiler_version = "0.1.0"` -> `"0.1.0+meta"`.
   2. Expected: cache invalid, rebuild required.
2. Change only `game_version` string:
   1. Example: `game_version = "0.1.0"` -> `"0.1.0-hotfix"`.
   2. Expected: cache invalid, rebuild required.
3. Rename mod folder (thus `mod_id` changes):
   1. Example: `betterlabels` -> `better_labels`.
   2. Expected: cache invalid, rebuild required.
