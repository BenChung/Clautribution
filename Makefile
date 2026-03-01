DIST_DIR := dist/clautribution

.PHONY: build test package clean

build:
	cargo build --release

test:
	cargo test

package: build
	mkdir -p $(DIST_DIR)/bin $(DIST_DIR)/.claude-plugin $(DIST_DIR)/hooks
	cp target/release/clautribution $(DIST_DIR)/bin/clautribution
	cp .claude-plugin/plugin.json $(DIST_DIR)/.claude-plugin/plugin.json
	cp .claude-plugin/marketplace.json $(DIST_DIR)/.claude-plugin/marketplace.json
	sed 's|target/release/clautribution|bin/clautribution|' hooks/hooks.json > $(DIST_DIR)/hooks/hooks.json
	cd dist && zip -r clautribution-plugin.zip clautribution/

clean:
	cargo clean
	rm -rf dist
