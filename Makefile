VERSION=0.20220917
NS=

build:
	cargo build

image:
	mkdir -p $$(pwd)/,container-cache && chmod 777 $$(pwd)/,container-cache
	buildah bud --layers -f Dockerfile -v $$(pwd)/,container-cache:/target -t loadwork:$(VERSION) .
	buildah tag loadwork:$(VERSION) loadwork:latest
	buildah tag loadwork:latest microk8s:32000/loadwork:latest
	buildah push microk8s:32000/loadwork:latest

test:
	$(MAKE) doctest
	$(MAKE) test-cargo

doctest:
	@RUST_BUCKTRACE=1 cargo test --doc -- --nocapture

test-cargo:
	NS=test-loadwork-cargo; \
	@kubectl get ns $$NS >/dev/null 2>&1 || $(MAKE) test-k8s-setup NS=$$NS; \
	. ../../test/env.sh; \
	export LW_INDIR=/tmp/loadwork-test/in; \
	export LW_OUTDIR=/tmp/loadwork-test/out; \
	export LW_TARGET_ID=loadwork-test; \
	export LW_WORK_NAME=cargo; \
	export LW_WORK_VERSION=1; \
	rm -rf /tmp/loadwork-test; mkdir -p /tmp/loadwork-test; \
	cargo test -- --nocapture

test-k8s:
	@kubectl get ns test-loadwork >/dev/null 2>&1 || $(MAKE) test-k8s-setup NS=test-loadwork
	kubectl -n test-demucs wait pod --for=condition=ready --selector app=mongodb
	kubectl -n test-loadwork delete -f ./test.yaml 2>/dev/null || true
	kubectl -n test-loadwork apply -f ./test.yaml
	kubectl -n test-loadwork wait jobs --for=condition=complete --selector app=loadwork
	kubectl -n test-loadwork logs --selector app=loadwork -c results --tail=100

test-k8s-setup:
	$(MAKE) -C ../../test NS=$(NS) clean
	$(MAKE) -C ../../test NS=$(NS) backend

test-k8s-clean:
	$(MAKE) -C ../../test NS=$(NS) clean

clean:
	find . -name "*~" -exec rm {} \;

.PHONY: build test doctest itest image
