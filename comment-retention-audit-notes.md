# Comment Retention Audit Notes

## david/move-GenericServerObject-to-warp_server_client
- Restored four helper doc comments verbatim on `TryFromGql::try_from_gql` implementations in `app/src/cloud_object/mod.rs`. These comments originally described `try_from_graphql_fields` helpers, so the wording is somewhat stale after the refactor, but I preserved it exactly per the audit goal.

## david/move-GenericCloudObject-to-warp_server_client
- Restored the `new_from_server` doc comment verbatim even though the parameter type is now spelled `GenericServerObject`, because the pre-move comment used the broader `ServerObject` wording.
- Restored `// Import inline because of circular dependencies` above the new top-level `CloudMCPServer` import in `app/src/ai/mcp/templatable_manager/native.rs`. The import is no longer inline after the refactor, but I preserved the original wording rather than rewriting it.
- Did not restore `/// Returns a bulk upsert event for putting a list of this model into the SQLite database.` because the corresponding `bulk_upsert_event(objects: &[Self])` method no longer exists on this branch; this looked like deleted functionality rather than moved code.

## david/create-cloud-objects-crate
- Left crate-name references updated from `warp_server_client` to `cloud_objects` in comments that moved with the crate split. I treated these as intentional semantic crate-rename updates rather than comment-retention regressions.
