#!/usr/bin/env bash
set -euo pipefail

protocol_root="${1:-../task-packet-system}"
target_root="crates/task-packet-protocol/schemas"

schemas=(
  adapter-manifest.schema.json
  context-slice.schema.json
  execution-provenance.schema.json
  integration-ledger.schema.json
  result-envelope.schema.json
  task-packet.schema.json
)

for schema in "${schemas[@]}"; do
  cp "${protocol_root}/schemas/${schema}" "${target_root}/${schema}"
  sed -i '${/^$/d;}' "${target_root}/${schema}"
done

(
  cd crates/task-packet-protocol
  sha256sum "${schemas[@]/#/schemas/}"
) > "${target_root}/schema-checksums.txt"
