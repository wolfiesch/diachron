#!/usr/bin/env bash
# ============================================================================
# Diachron Installer
# Automated setup for Diachron provenance tracking in Claude Code
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/wolfiesch/diachron/main/install.sh | bash
#
# Options:
#   --uninstall    Remove Diachron completely
#   --update       Update to latest version
#   --doctor       Run diagnostics
#   --help         Show this help message
#
# ============================================================================

set -e  # Exit on error

# ============================================================================
# Configuration
# ============================================================================

REPO_URL="https://github.com/wolfiesch/diachron"
INSTALL_DIR="$HOME/.claude/skills/diachron"
SETTINGS_FILE="$HOME/.claude/settings.json"
MIN_PYTHON_VERSION="3.8"

# ============================================================================
# Colors and Output Helpers
# ============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m' # No Color

print_header() {
    echo ""
    echo -e "${BOLD}${BLUE}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${BOLD}${BLUE}  $1${NC}"
    echo -e "${BOLD}${BLUE}═══════════════════════════════════════════════════════════${NC}"
    echo ""
}

print_step() {
    echo -e "${BLUE}▶${NC} $1"
}

print_success() {
    echo -e "${GREEN}✅${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠️${NC}  $1"
}

print_error() {
    echo -e "${RED}❌${NC} $1"
}

print_info() {
    echo -e "${BLUE}ℹ️${NC}  $1"
}

error_exit() {
    print_error "$1"
    echo ""
    echo "For help, run: install.sh --doctor"
    echo "Or visit: $REPO_URL/issues"
    exit 1
}

# ============================================================================
# Help
# ============================================================================

show_help() {
    cat << EOF
Diachron Installer - Agentic Provenance for Claude Code

USAGE:
    install.sh [OPTIONS]

OPTIONS:
    (no args)     Install or update Diachron
    --uninstall   Remove Diachron completely (preserves per-project data)
    --update      Update to latest version and rebuild
    --doctor      Run diagnostics and check installation health
    --help        Show this help message

EXAMPLES:
    # Fresh install
    curl -fsSL https://raw.githubusercontent.com/wolfiesch/diachron/main/install.sh | bash

    # Update existing installation
    ~/.claude/skills/diachron/install.sh --update

    # Check installation status
    ~/.claude/skills/diachron/install.sh --doctor

    # Remove completely
    ~/.claude/skills/diachron/install.sh --uninstall

REQUIREMENTS:
    - Claude Code 2.1+ (for PostToolUse hook support)
    - Python 3.8+
    - macOS or Linux (Windows untested)

MORE INFO:
    $REPO_URL

EOF
    exit 0
}

# ============================================================================
# Prerequisites Checking
# ============================================================================

check_prerequisites() {
    print_step "Checking prerequisites..."

    # Check Python version
    if ! command -v python3 &> /dev/null; then
        error_exit "Python 3 not found. Please install Python 3.8 or later."
    fi

    PYTHON_VERSION=$(python3 -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")')
    PYTHON_MAJOR=$(echo "$PYTHON_VERSION" | cut -d. -f1)
    PYTHON_MINOR=$(echo "$PYTHON_VERSION" | cut -d. -f2)

    if [[ "$PYTHON_MAJOR" -lt 3 ]] || [[ "$PYTHON_MAJOR" -eq 3 && "$PYTHON_MINOR" -lt 8 ]]; then
        error_exit "Python $PYTHON_VERSION found, but $MIN_PYTHON_VERSION+ required."
    fi
    print_success "Python $PYTHON_VERSION"

    # Check Claude Code directory
    if [ ! -d "$HOME/.claude" ]; then
        error_exit "Claude Code not found (~/.claude doesn't exist). Please install Claude Code first."
    fi
    print_success "Claude Code directory found"

    # Check settings.json
    if [ ! -f "$SETTINGS_FILE" ]; then
        error_exit "settings.json not found. Have you run Claude Code at least once?"
    fi
    print_success "settings.json found"

    # Check git
    if ! command -v git &> /dev/null; then
        error_exit "git not found. Please install git."
    fi
    print_success "git available"

    echo ""
}

# ============================================================================
# Architecture Detection
# ============================================================================

detect_architecture() {
    print_step "Detecting system architecture..."

    ARCH=$(uname -m)
    OS=$(uname -s)

    echo "  OS: $OS"
    echo "  Architecture: $ARCH"

    # Determine hook type
    if [[ "$OS" == "Darwin" && "$ARCH" == "arm64" ]]; then
        # macOS ARM64 - pre-built binary available
        HOOK_TYPE="rust"
        print_success "macOS ARM64 detected - will use pre-built Rust binary"
    else
        # Other platforms - check if we can build Rust
        if command -v cargo &> /dev/null; then
            HOOK_TYPE="rust_build"
            print_info "Rust toolchain found - will build from source"
        else
            HOOK_TYPE="python"
            print_warning "No pre-built binary for $OS/$ARCH and Rust not available"
            print_info "Will use Python fallback (~300ms vs ~12ms latency)"
        fi
    fi

    # Check for optional OpenAI key
    if [[ -n "$OPENAI_API_KEY" ]]; then
        print_info "OpenAI API key detected (AI summaries available)"
    fi

    echo ""
}

# ============================================================================
# Installation
# ============================================================================

install_diachron() {
    print_step "Installing Diachron..."

    if [ -d "$INSTALL_DIR/.git" ]; then
        # Existing installation - update
        print_info "Existing installation found, updating..."
        cd "$INSTALL_DIR"

        # Check for local changes
        if ! git diff-index --quiet HEAD -- 2>/dev/null; then
            print_warning "Local changes detected, stashing..."
            git stash
        fi

        git pull origin main 2>/dev/null || git pull origin master 2>/dev/null || {
            print_warning "Could not pull latest. Continuing with existing version."
        }
        print_success "Updated to latest version"
    elif [ -d "$INSTALL_DIR" ]; then
        # Directory exists but not a git repo (manual install?)
        print_warning "Existing non-git installation found at $INSTALL_DIR"
        print_info "Backing up and replacing..."
        mv "$INSTALL_DIR" "${INSTALL_DIR}.backup.$(date +%Y%m%d_%H%M%S)"
        git clone "$REPO_URL" "$INSTALL_DIR"
        print_success "Fresh installation complete"
    else
        # Fresh install
        print_info "Cloning from $REPO_URL..."
        mkdir -p "$(dirname "$INSTALL_DIR")"
        git clone "$REPO_URL" "$INSTALL_DIR"
        print_success "Installation complete"
    fi

    # Verify critical files
    print_step "Verifying installation structure..."
    local required_files=(
        "diachron.md"
        "timeline.md"
        "lib/db.py"
        "lib/hook_capture.py"
    )

    for file in "${required_files[@]}"; do
        if [ ! -f "$INSTALL_DIR/$file" ]; then
            error_exit "Missing required file: $file"
        fi
    done
    print_success "All required files present"

    echo ""
}

# ============================================================================
# Hook Building
# ============================================================================

build_rust_hook() {
    local RUST_DIR="$INSTALL_DIR/rust"
    local BINARY_PATH="$RUST_DIR/target/release/diachron-hook"

    if [[ "$HOOK_TYPE" == "rust" ]]; then
        # Check if pre-built binary exists and works
        if [ -f "$BINARY_PATH" ]; then
            print_step "Testing pre-built Rust binary..."
            if echo '{}' | "$BINARY_PATH" 2>/dev/null; then
                print_success "Pre-built binary works"
                # Use $HOME instead of ~ for proper expansion in settings.json
                HOOK_CMD="$HOME/.claude/skills/diachron/rust/target/release/diachron-hook"
                return 0
            else
                print_warning "Pre-built binary failed, will attempt rebuild"
                HOOK_TYPE="rust_build"
            fi
        else
            print_info "Pre-built binary not found"
            if command -v cargo &> /dev/null; then
                HOOK_TYPE="rust_build"
            else
                HOOK_TYPE="python"
            fi
        fi
    fi

    if [[ "$HOOK_TYPE" == "rust_build" ]]; then
        print_step "Building Rust hook from source..."

        if [ ! -f "$RUST_DIR/Cargo.toml" ]; then
            print_warning "Rust source not found, falling back to Python"
            HOOK_TYPE="python"
        else
            cd "$RUST_DIR"

            # Clean build for consistency
            cargo clean 2>/dev/null || true

            # Capture build output for debugging if it fails
            local build_output
            if build_output=$(cargo build --release 2>&1); then
                # Verify the build
                if [ -f "$BINARY_PATH" ] && echo '{}' | "$BINARY_PATH" 2>/dev/null; then
                    print_success "Rust hook built successfully"
                    # Use $HOME instead of ~ for proper expansion in settings.json
                    HOOK_CMD="$HOME/.claude/skills/diachron/rust/target/release/diachron-hook"
                    HOOK_TYPE="rust"
                    return 0
                else
                    print_warning "Rust build succeeded but binary test failed"
                    print_info "Build output:"
                    echo "$build_output"
                    HOOK_TYPE="python"
                fi
            else
                print_warning "Rust build failed, falling back to Python"
                print_info "Build output:"
                echo "$build_output"
                HOOK_TYPE="python"
            fi
        fi
    fi

    if [[ "$HOOK_TYPE" == "python" ]]; then
        print_step "Configuring Python hook fallback..."
        # Use $HOME instead of ~ for proper expansion in settings.json
        HOOK_CMD="python3 $HOME/.claude/skills/diachron/lib/hook_capture.py"

        # Verify Python hook works
        if echo '{}' | python3 "$INSTALL_DIR/lib/hook_capture.py" 2>/dev/null; then
            print_success "Python hook configured"
            print_info "Note: Python hook adds ~300ms latency per operation"
            print_info "Install Rust (rustup.rs) for better performance"
        else
            error_exit "Python hook failed to execute"
        fi
    fi

    echo ""
}

# ============================================================================
# Settings Configuration
# ============================================================================

configure_settings() {
    print_step "Configuring Claude Code settings..."

    # Create backup
    local BACKUP_FILE="$HOME/.claude/settings.json.backup.$(date +%Y%m%d_%H%M%S)"
    cp "$SETTINGS_FILE" "$BACKUP_FILE"
    print_info "Backed up settings to: $BACKUP_FILE"

    # Use Python for reliable JSON manipulation
    python3 << EOF
import json
from pathlib import Path

settings_path = Path("$SETTINGS_FILE")
settings = json.loads(settings_path.read_text())

# Initialize hooks if needed
if "hooks" not in settings:
    settings["hooks"] = {}
if "PostToolUse" not in settings["hooks"]:
    settings["hooks"]["PostToolUse"] = []

# Define the Diachron hook
diachron_hook = {
    "matcher": "Write|Edit|Bash",
    "hooks": [{
        "type": "command",
        "command": "$HOOK_CMD",
        "timeout": 5
    }]
}

# Remove any existing Diachron hooks to avoid duplicates
existing = settings["hooks"]["PostToolUse"]
settings["hooks"]["PostToolUse"] = [
    h for h in existing
    if "diachron" not in str(h.get("hooks", [])).lower()
]

# Add the new Diachron hook
settings["hooks"]["PostToolUse"].append(diachron_hook)

# Write back with proper formatting
settings_path.write_text(json.dumps(settings, indent=2))
print("Hook configuration updated")
EOF

    if [ $? -eq 0 ]; then
        print_success "PostToolUse hook configured"
    else
        print_warning "Could not update settings automatically"
        print_info "Please add the hook manually. See: $REPO_URL/blob/main/INSTALL.md"
    fi

    echo ""
}

# ============================================================================
# Verification
# ============================================================================

verify_installation() {
    print_step "Verifying installation..."

    local all_good=true

    # Check skill files
    if [ -d "$INSTALL_DIR" ]; then
        print_success "Skill directory: $INSTALL_DIR"
    else
        print_error "Skill directory not found"
        all_good=false
    fi

    # Check hook in settings
    if grep -q "diachron" "$SETTINGS_FILE" 2>/dev/null; then
        print_success "Hook registered in settings.json"
    else
        print_error "Hook not found in settings.json"
        all_good=false
    fi

    # Test hook execution
    if [[ "$HOOK_TYPE" == "rust" ]]; then
        local binary="$HOME/.claude/skills/diachron/rust/target/release/diachron-hook"
        if [ -f "$binary" ] && echo '{}' | "$binary" 2>/dev/null; then
            print_success "Rust hook executes successfully"
        else
            print_warning "Rust hook execution test failed"
        fi
    else
        if echo '{}' | python3 "$INSTALL_DIR/lib/hook_capture.py" 2>/dev/null; then
            print_success "Python hook executes successfully"
        else
            print_warning "Python hook execution test failed"
        fi
    fi

    echo ""

    if $all_good; then
        return 0
    else
        return 1
    fi
}

# ============================================================================
# Uninstall
# ============================================================================

uninstall_diachron() {
    print_header "Uninstalling Diachron"

    # Remove hook from settings.json
    print_step "Removing hook from settings..."

    if [ -f "$SETTINGS_FILE" ]; then
        python3 << 'EOF'
import json
from pathlib import Path
import sys

settings_path = Path.home() / ".claude" / "settings.json"
if not settings_path.exists():
    print("Settings file not found")
    sys.exit(0)

settings = json.loads(settings_path.read_text())

if "hooks" in settings and "PostToolUse" in settings["hooks"]:
    original_count = len(settings["hooks"]["PostToolUse"])
    settings["hooks"]["PostToolUse"] = [
        h for h in settings["hooks"]["PostToolUse"]
        if "diachron" not in str(h.get("hooks", [])).lower()
    ]
    new_count = len(settings["hooks"]["PostToolUse"])

    if original_count != new_count:
        settings_path.write_text(json.dumps(settings, indent=2))
        print(f"Removed {original_count - new_count} Diachron hook(s)")
    else:
        print("No Diachron hooks found in settings")
else:
    print("No PostToolUse hooks configured")
EOF
        print_success "Hook removed from settings"
    fi

    # Remove skill directory
    print_step "Removing skill files..."
    if [ -d "$INSTALL_DIR" ]; then
        rm -rf "$INSTALL_DIR"
        print_success "Skill directory removed"
    else
        print_info "Skill directory not found (already removed?)"
    fi

    print_header "Uninstall Complete"
    echo "Note: Per-project .diachron/ directories are preserved."
    echo "To remove them, delete .diachron/ from each project manually."
    echo ""
    exit 0
}

# ============================================================================
# Update
# ============================================================================

update_diachron() {
    print_header "Updating Diachron"

    if [ ! -d "$INSTALL_DIR" ]; then
        error_exit "Diachron not installed. Run without --update to install."
    fi

    cd "$INSTALL_DIR"

    print_step "Pulling latest changes..."
    git pull origin main 2>/dev/null || git pull origin master 2>/dev/null || {
        error_exit "Could not pull latest changes"
    }
    print_success "Updated to latest version"

    # Rebuild Rust if available
    if [ -f "$INSTALL_DIR/rust/Cargo.toml" ] && command -v cargo &> /dev/null; then
        print_step "Rebuilding Rust hook..."
        cd "$INSTALL_DIR/rust"
        cargo build --release 2>&1 && print_success "Rust hook rebuilt" || print_warning "Rust rebuild failed"
    fi

    print_header "Update Complete"
    echo "Restart Claude Code to apply changes."
    echo ""
    exit 0
}

# ============================================================================
# Doctor/Diagnostics
# ============================================================================

run_doctor() {
    print_header "Diachron Diagnostics"

    local issues=0

    # System info
    echo "System Information:"
    echo "  OS: $(uname -s)"
    echo "  Arch: $(uname -m)"
    echo "  Python: $(python3 --version 2>/dev/null || echo 'Not found')"
    echo "  Rust: $(cargo --version 2>/dev/null || echo 'Not installed')"
    echo ""

    # Installation checks
    echo "Installation Status:"

    echo -n "  Skill installed: "
    if [ -d "$INSTALL_DIR" ]; then
        echo -e "${GREEN}✅${NC}"
    else
        echo -e "${RED}❌${NC}"
        ((issues++))
    fi

    echo -n "  Hook configured: "
    if grep -q "diachron" "$SETTINGS_FILE" 2>/dev/null; then
        echo -e "${GREEN}✅${NC}"
    else
        echo -e "${RED}❌${NC}"
        ((issues++))
    fi

    echo -n "  Rust binary exists: "
    if [ -f "$INSTALL_DIR/rust/target/release/diachron-hook" ]; then
        echo -e "${GREEN}✅${NC}"
    else
        echo -e "${YELLOW}⚠️${NC} (using Python fallback)"
    fi

    echo -n "  Rust binary works: "
    if [ -f "$INSTALL_DIR/rust/target/release/diachron-hook" ]; then
        if echo '{}' | "$INSTALL_DIR/rust/target/release/diachron-hook" 2>/dev/null; then
            echo -e "${GREEN}✅${NC}"
        else
            echo -e "${RED}❌${NC}"
            ((issues++))
        fi
    else
        echo -e "${YELLOW}N/A${NC}"
    fi

    echo -n "  Python hook works: "
    if [ -f "$INSTALL_DIR/lib/hook_capture.py" ]; then
        if echo '{}' | python3 "$INSTALL_DIR/lib/hook_capture.py" 2>/dev/null; then
            echo -e "${GREEN}✅${NC}"
        else
            echo -e "${RED}❌${NC}"
            ((issues++))
        fi
    else
        echo -e "${RED}❌${NC} (file not found)"
        ((issues++))
    fi

    echo -n "  Python 3.8+: "
    if python3 -c "import sys; exit(0 if sys.version_info >= (3,8) else 1)" 2>/dev/null; then
        echo -e "${GREEN}✅${NC}"
    else
        echo -e "${RED}❌${NC}"
        ((issues++))
    fi

    echo -n "  OpenAI API key: "
    if [[ -n "$OPENAI_API_KEY" ]]; then
        echo -e "${GREEN}✅${NC} (AI summaries available)"
    else
        echo -e "${YELLOW}⚠️${NC} (optional, for /timeline --summarize)"
    fi

    echo ""

    # Hook configuration details
    echo "Hook Configuration:"
    if grep -q "diachron" "$SETTINGS_FILE" 2>/dev/null; then
        grep -A 5 "diachron" "$SETTINGS_FILE" | head -10
    else
        echo "  No Diachron hook found in settings.json"
    fi

    echo ""
    echo "═══════════════════════════════════════════════════════════"

    if [ $issues -eq 0 ]; then
        echo -e "${GREEN}All checks passed!${NC}"
    else
        echo -e "${YELLOW}Found $issues issue(s). Run installer to fix.${NC}"
    fi

    echo ""
    exit 0
}

# ============================================================================
# Main Installation Flow
# ============================================================================

main_install() {
    print_header "Diachron Installer"

    check_prerequisites
    detect_architecture
    install_diachron
    build_rust_hook
    configure_settings
    verify_installation

    print_header "Installation Complete!"

    echo "Hook type: ${BOLD}$HOOK_TYPE${NC}"
    echo "Location: $INSTALL_DIR"
    echo ""
    echo -e "${YELLOW}⚠️  IMPORTANT: Restart Claude Code to activate the hook${NC}"
    echo ""
    echo "Next steps:"
    echo "  1. Restart Claude Code (close and reopen)"
    echo "  2. Open a project and run: /diachron init"
    echo "  3. Make some changes, then run: /timeline"
    echo ""
    echo "Commands available after restart:"
    echo "  /diachron init    - Initialize tracking for current project"
    echo "  /diachron status  - Check tracking status"
    echo "  /timeline         - View change history"
    echo "  /timeline --stats - Show statistics"
    echo ""
    echo "For help: $REPO_URL"
    echo ""
}

# ============================================================================
# Entry Point
# ============================================================================

case "${1:-}" in
    --help|-h)
        show_help
        ;;
    --uninstall)
        uninstall_diachron
        ;;
    --update)
        update_diachron
        ;;
    --doctor)
        run_doctor
        ;;
    *)
        main_install
        ;;
esac
