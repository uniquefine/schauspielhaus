.PHONY: test local proxy-prod-db

test:
	cargo test

local:
	docker compose -f compose.yml up -d
	op run --env-file=.env -- cargo run -- start

proxy-prod-db:
	fly proxy 55432:5432 -a frosty-voice-1550

migrations-prod:
	op run --env-file=prod-env -- diesel setup
	op run --env-file=prod-env -- diesel migration run

scrape-prod:
	op run --env-file=prod-env -- cargo run -- scrape
