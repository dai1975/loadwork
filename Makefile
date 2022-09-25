SHELL=/bin/bash
VERSION=0.20220925
TESTENV=.env.test
CONTAINER_REGISTRY=192.168.39.2:32000

build:
	cargo build

image:
	mkdir -p $$(pwd)/,container-cache && chmod 777 $$(pwd)/,container-cache
	buildah bud --layers -f Dockerfile -v $$(pwd)/,container-cache:/target -t loadwork:$(VERSION) .
	buildah tag loadwork:$(VERSION) loadwork:latest
	buildah tag loadwork:latest $(CONTAINER_REGISTRY)/loadwork:latest
	buildah push $(CONTAINER_REGISTRY)/loadwork:latest

test:
	$(MAKE) test-doc

test-doc:
	@RUST_BUCKTRACE=1 cargo test --doc -- --nocapture

test-local:
	@. .env.test; kubectl get ns $$X_K8S_NS >/dev/null 2>&1 || $(MAKE) test-k8s-setup
	. .env.test; \
	export LW_TARGET_ID=test-local; \
	export LW_WORK_NAME=foobar; \
	export LW_WORK_VERSION=1; \
	export LW_INDIR=/tmp/lw-${LW_TARGET_ID}-in; \
	export LW_OUTDIR=/tmp/lw-${LW_TARGET_ID}-out; \
	test -e $$LW_INDIR && rm -rf $$LW_INDIR; \
	test -e $$LW_OUTDIR && rm -rf $$LW_OUTDIR; \
	cargo test -- --nocapture

test-k8s: tests/k8s-test.yaml
	@. .env.test; kubectl get ns $$X_K8S_NS >/dev/null 2>&1 || $(MAKE) test-k8s-setup
	. .env.test; kubectl -n $$X_K8S_NS delete -f $< 2>/dev/null || true
	. .env.test; kubectl -n $$X_K8S_NS apply -f $<
	. .env.test; kubectl -n $$X_K8S_NS wait jobs --for=condition=complete --selector app=loadwork
	. .env.test; kubectl -n $$X_K8S_NS logs --selector app=loadwork -c results --tail=100

test-k8s-setup: tests/k8s-depends.yaml
	kubectl apply -f $<

tests/k8s-depends.yaml: tests/k8s-depends.tmpl.yaml $(TESTENV)
	. $(TESTENV) && envsubst < $< > $@

tests/k8s-test.yaml: tests/k8s-test.tmpl.yaml $(TESTENV)
	. $(TESTENV) && envsubst < $< > $@

test-k8s-clean: $(TESTENV)
	. $(TESTENV) && kubectl delete namespace $$X_K8S_NS

clean:
	find . -name "*~" -exec rm {} \;

.PHONY: build test doctest itest image
