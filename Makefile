.PHONY: test local proxy-prod-db

test:
	cargo test

local:
	docker compose up -f compose.yml -d
	cargo start

proxy-prod-db:
	fly proxy 55432:5432 -a frosty-voice-1550

migrations-prod:
	op run --env-file=prod-env -- diesel setup

scrape-prod:
	op run --env-file=prod-env -- cargo run -- scrape
