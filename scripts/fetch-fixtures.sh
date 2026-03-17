#!/usr/bin/env bash
# Shallow-clone integration test fixture repositories.
# Each repo is fetched at a pinned commit with --depth=1.
set -euo pipefail

DEST="vendor/cspell/integration-tests/repositories/temp"
mkdir -p "$DEST"

clone_repo() {
    local path="$1" url="$2" commit="$3"
    local dir="$DEST/$path"
    if [ -d "$dir/.git" ]; then
        local head
        head="$(git -C "$dir" rev-parse HEAD 2>/dev/null || true)"
        if [ "$head" = "$commit" ]; then
            echo "skip  $path (ready @ ${commit:0:12})"
            return
        fi
        echo "refresh $path (expected ${commit:0:12}, got ${head:0:12})"
        rm -rf "$dir"
    elif [ -d "$dir" ]; then
        echo "refresh $path (non-git directory)"
        rm -rf "$dir"
    fi
    echo "clone $path @ ${commit:0:12}"
    mkdir -p "$dir"
    git -C "$dir" init -q
    git -C "$dir" remote add origin "$url"
    git -C "$dir" fetch -q --depth=1 origin "$commit"
    git -C "$dir" checkout -q --detach FETCH_HEAD
}

# cspell:disable
clone_repo "AdaDoom3/AdaDoom3"          "https://github.com/AdaDoom3/AdaDoom3.git"            "6b979248c03cfae9af705257b116179f7d250267"
clone_repo "Azure/azure-rest-api-specs" "https://github.com/Azure/azure-rest-api-specs.git"   "1d03fa5b7fa3f86eef71ef4433a474cb14d96443"
clone_repo "MartinThoma/LaTeX-examples" "https://github.com/MartinThoma/LaTeX-examples.git"   "2286e6e3833904b2c058b2a855db9b7f81776c59"
clone_repo "MicrosoftDocs/PowerShell-Docs" "https://github.com/MicrosoftDocs/PowerShell-Docs.git" "75aaa4850e97816fde550f59af250677f7c9792b"
clone_repo "RustPython/RustPython"      "https://github.com/RustPython/RustPython.git"        "c974b77127307e24c2c9926f5434498ee1f78df1"
clone_repo "dart-lang/sdk"              "https://github.com/dart-lang/sdk"                     "2076ac5df015b4929317f7a99f1d5430943f8a7c"
clone_repo "flutter/samples"            "https://github.com/flutter/samples"                   "54106b0bf6d90bb9ce6fd66e4473be390faae78d"
clone_repo "gitbucket/gitbucket"        "https://github.com/gitbucket/gitbucket.git"           "3826b690cfba07b966c15c4a298171c21f6703d8"
clone_repo "liriliri/licia"             "https://github.com/liriliri/licia.git"                "ce21a9276e39aaedcbcec71a8e17871e23beeae9"
clone_repo "prettier/prettier"          "https://github.com/prettier/prettier.git"             "f7a1bcfe3344dcc463f24d97da9a5c12d9909e0d"
clone_repo "vitest-dev/vitest"          "https://github.com/vitest-dev/vitest"                 "3425e28fa01559da96ef7e91593b6f16eb05d200"
clone_repo "wireapp/wire-webapp"        "https://github.com/wireapp/wire-webapp.git"           "dd5c2dba10fea73af3ef570cea71b7b426d7ea47"

echo "done: all fixture repositories ready"
