build-LlamaCppServer:
	docker build -t llama-cpp-server .
	docker create --name temp_container llama-cpp-server
	docker cp temp_container:/opt/. $(ARTIFACTS_DIR)/
	docker rm temp_container
	cp run.sh $(ARTIFACTS_DIR)/
