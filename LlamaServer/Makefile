build-LlamaServer:
	docker build -t llama-server .
	docker create --name temp_container llama-server
	docker cp temp_container:/opt/. $(ARTIFACTS_DIR)/
	docker rm temp_container
	cp run.sh $(ARTIFACTS_DIR)/
