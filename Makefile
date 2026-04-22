install-hooks:
	chmod +x .githooks/pre-commit .githooks/pre-push scripts/*.sh
	git config core.hooksPath .githooks
	@echo "Installed git hooks from .githooks"
