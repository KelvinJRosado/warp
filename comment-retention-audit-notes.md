# Comment Retention Audit Notes

## david/move-GenericServerObject-to-warp_server_client
- Restored four helper doc comments verbatim on `TryFromGql::try_from_gql` implementations in `app/src/cloud_object/mod.rs`. These comments originally described `try_from_graphql_fields` helpers, so the wording is somewhat stale after the refactor, but I preserved it exactly per the audit goal.
