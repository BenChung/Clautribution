DIST_DIR := dist/claudtributter

.PHONY: build test package clean

build:
	cargo build --release

test:
	cargo test

package: build
	mkdir -p $(DIST_DIR)/bin $(DIST_DIR)/.claude-plugin $(DIST_DIR)/hooks
	cp target/release/claudtributter $(DIST_DIR)/bin/claudtributter
	cp .claude-plugin/plugin.json $(DIST_DIR)/.claude-plugin/plugin.json
	sed 's|target/release/claudtributter|bin/claudtributter|' hooks/hooks.json > $(DIST_DIR)/hooks/hooks.json
	cd dist && zip -r claudtributter-plugin.zip claudtributter/

clean:
	cargo clean
	rm -rf dist
