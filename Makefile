TERMINAL_MCP_REPO := https://github.com/iris-networks/terminal_mcp.git
TERMINAL_MCP_HASH := 73f111d580bd5c64a8d8742e4be5bc4698794234
TERMINAL_MCP_INSTALL_DIR := $(HOME)/.agent-deck/mcp/terminalmcp
TERMINAL_MCP_BUILD_DIR := $(TERMINAL_MCP_INSTALL_DIR)/.build
TERMINAL_MCP_BINARY := $(TERMINAL_MCP_INSTALL_DIR)/terminal_mcp/mcp-terminal-server

.PHONY: install-terminal-mcp update-terminal-mcp _build-terminal-mcp

## install-terminal-mcp: Build and install the terminal MCP server (skips if binary already exists)
install-terminal-mcp:
	@if [ -f "$(TERMINAL_MCP_BINARY)" ]; then \
		echo "terminal MCP already installed at $(TERMINAL_MCP_BINARY)"; \
		echo "Run 'make update-terminal-mcp' to force a rebuild."; \
	else \
		$(MAKE) _build-terminal-mcp; \
	fi

## update-terminal-mcp: Force rebuild of the terminal MCP server (even if binary exists)
update-terminal-mcp: _build-terminal-mcp

_build-terminal-mcp:
	@command -v git >/dev/null 2>&1 || { echo "ERROR: git is not installed."; \
		case "$$(uname)" in \
			Darwin) echo "Install with: brew install git  OR download from https://git-scm.com/downloads";; \
			Linux)  echo "Install with: sudo apt install git  OR download from https://git-scm.com/downloads";; \
			*)      echo "Download from: https://git-scm.com/downloads";; \
		esac; exit 1; }
	@command -v go >/dev/null 2>&1 || { echo "ERROR: Go is not installed."; \
		case "$$(uname)" in \
			Darwin) echo "Install with: brew install go  OR download from https://go.dev/dl/";; \
			Linux)  echo "Install with: sudo apt install golang-go  OR download from https://go.dev/dl/";; \
			*)      echo "Download from: https://go.dev/dl/";; \
		esac; exit 1; }
	@echo "Cloning terminal MCP repository..."
	@mkdir -p "$(TERMINAL_MCP_BUILD_DIR)"
	@if [ -d "$(TERMINAL_MCP_BUILD_DIR)/.git" ]; then \
		cd "$(TERMINAL_MCP_BUILD_DIR)" && git fetch --quiet; \
	else \
		git clone --quiet "$(TERMINAL_MCP_REPO)" "$(TERMINAL_MCP_BUILD_DIR)"; \
	fi
	@cd "$(TERMINAL_MCP_BUILD_DIR)" && git checkout --quiet "$(TERMINAL_MCP_HASH)"
	@echo "Building terminal MCP server (this may take a minute)..."
	@mkdir -p "$(TERMINAL_MCP_INSTALL_DIR)/terminal_mcp"
	@cd "$(TERMINAL_MCP_BUILD_DIR)" && go build -o "$(TERMINAL_MCP_BINARY)" .
	@chmod +x "$(TERMINAL_MCP_BINARY)"
	@echo "Terminal MCP server installed at $(TERMINAL_MCP_BINARY)"
