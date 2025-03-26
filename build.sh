#!/usr/bin/env bash

rm -rf build
mkdir build

docker build -t lethal-mod-tinder . --progress=plain --no-cache
docker run --mount type=bind,src=./build,dst=/build/target lethal-mod-tinder
