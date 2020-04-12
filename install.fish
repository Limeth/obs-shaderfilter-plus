#!/usr/bin/env fish
set DIR (realpath (dirname (status --current-filename)))

if not cargo build --release
    echo "Could not build the plugin; not installing." 1>&2
    exit 1
end

if not test -f "$DIR/target/release/libobs_shaderfilter_plus.so"
    echo "The binary was not built; aborting." 1>&2
    exit 1
end

sudo cp "$DIR/target/release/libobs_shaderfilter_plus.so" /usr/lib/obs-plugins/libobs-shaderfilter-plus.so
