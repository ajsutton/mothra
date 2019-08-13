SHELL := /bin/sh

include config.mk

.PHONY:=c java c-bindings java-bindings c-examples java-examples clean

c-mash: clean c

java-mash: clean java

c: c-examples

java: java-examples

c-examples: c-bindings
	@echo ""
	@echo Building examples
	cd $(EXAMPLES_DIR) && make $@

java-examples: java-bindings
	@echo ""
	@echo Building examples
	cd $(EXAMPLES_DIR) && make $@

c-bindings:
	 @echo ""
	 @echo Building C bindings
	 cd $(BIND_DIR) && make $@
	 @echo ""
	 @echo Building Rust bindings
	 cd $(CORE_DIR) && make $@

java-bindings: java-bindings-ingress java-bindings-egress

java-bindings-ingress:
	@echo ""
	@echo Building Java bindings
	cd $(BIND_DIR) && make $@
	@echo ""
	@echo Building Rust bindings
	cd $(CORE_DIR) && make $@

java-bindings-egress:
	@echo ""
	@echo Building Java bindings
	cd $(BIND_DIR) && make $@

clean:
	cd $(CORE_DIR) && make $@
	cd $(BIND_DIR) && make $@
	cd $(EXAMPLES_DIR) && make $@