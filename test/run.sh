#!/bin/bash

set -e

find "binaries" -type f -name build.sh -print0 | while IFS= read -r -d '' build; do
    dir="$(dirname "$build")"
    (
        set -xe
        cd "$dir"
        rm -f hello_world *.dylib
        bash ./build.sh
    )
done

 ./tester.sh -p execvm_example -m path