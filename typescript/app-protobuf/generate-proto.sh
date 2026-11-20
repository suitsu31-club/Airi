#!/usr/bin/env bash

# Generate TypeScript client code from the workspace protobuf definitions.
#
# Proto files live at the repository root in `proto/` (../../proto from this
# package). The generated code is written into `src/generated/` and is consumed
# by the frontend packages via the `app-protobuf` package. Run it with:
#
#   bun run generate:proto            # from this package
#   bun run --filter app-protobuf generate:proto   # from the repo root

set -euo pipefail
shopt -s globstar nullglob

# Always run relative to this package, regardless of the caller's cwd.
cd "$(dirname "$0")"

# Sanity check: make sure this is the right package.
if [ ! -f package.json ]; then
  echo "Error: package.json not found. Is this the correct working directory?"
  exit 1
fi
package_name=$(awk -F'"' '/"name"/ {print $4; exit}' package.json)
if [ "$package_name" != "app-protobuf" ]; then
  echo "Error: package name is '$package_name', expected 'app-protobuf'."
  exit 1
fi

PROTO_DIR="../../proto"
OUT_DIR="src/generated"

# Collect every .proto in the repository, at any depth.
protos=("$PROTO_DIR"/**/*.proto)

# Refresh the output directory.
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

# The template ships without any proto files. Exit cleanly in that case.
if [ ${#protos[@]} -eq 0 ]; then
  echo "No .proto files found under $PROTO_DIR; nothing to generate."
  echo "Add your definitions to the repository 'proto/' directory and re-run."
  exit 0
fi

# Generate. The tool binaries are provided by grpc-tools / ts-proto and are on
# PATH because this runs through 'bun run'.
grpc_tools_node_protoc \
  --plugin=protoc-gen-ts_proto="$(which protoc-gen-ts_proto)" \
  --ts_proto_out="$OUT_DIR" \
  --ts_proto_opt=outputServices=nice-grpc,outputServices=generic-definitions,useExactTypes=false,importSuffix=.js,forceLong=bigint,removeEnumPrefix=true,useOptionals=none,disableProto2Optionals=true \
  --proto_path="$PROTO_DIR" \
  "${protos[@]}"

echo "Generated ${#protos[@]} proto file(s) into $OUT_DIR."
