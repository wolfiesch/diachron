"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const diachronClient_1 = require("./diachronClient");
const hoverProvider_1 = require("./hoverProvider");
const decorations_1 = require("./decorations");
const sidebar_1 = require("./sidebar");
let client;
let decorationProvider;
let blameDetailsProvider;
async function activate(context) {
    console.log('Diachron extension activating...');
    // Initialize daemon client
    client = new diachronClient_1.DiachronClient();
    // Check daemon status
    const status = await client.status();
    if (!status.connected) {
        vscode.window.showWarningMessage('Diachron daemon not running. Run `diachron daemon start` to enable AI blame.', 'Start Daemon').then(selection => {
            if (selection === 'Start Daemon') {
                vscode.commands.executeCommand('diachron.startDaemon');
            }
        });
    }
    else {
        console.log(`Diachron connected: ${status.version}`);
    }
    // Register hover provider for all languages
    const hoverProvider = new hoverProvider_1.DiachronHoverProvider(client);
    context.subscriptions.push(vscode.languages.registerHoverProvider({ scheme: 'file' }, hoverProvider));
    // Register decoration provider for gutter icons
    decorationProvider = new decorations_1.DiachronDecorationProvider(client);
    context.subscriptions.push(decorationProvider);
    // Register sidebar providers (timeline + blame details)
    const sidebarProviders = (0, sidebar_1.registerSidebarProviders)(context, client);
    blameDetailsProvider = sidebarProviders.blameDetails;
    // Register commands
    context.subscriptions.push(vscode.commands.registerCommand('diachron.blame', async () => {
        const editor = vscode.window.activeTextEditor;
        if (!editor) {
            vscode.window.showErrorMessage('No active editor');
            return;
        }
        const line = editor.selection.active.line + 1;
        const filePath = editor.document.uri.fsPath;
        const content = editor.document.lineAt(editor.selection.active.line).text;
        const blame = await client.blame(filePath, line, content);
        if (blame) {
            // Update sidebar with blame details
            blameDetailsProvider.setBlame(blame);
            vscode.window.showInformationMessage(`${blame.event.tool_name} (${blame.confidence}): ${blame.intent || 'No intent recorded'}`);
        }
        else {
            blameDetailsProvider.clear();
            vscode.window.showInformationMessage('No AI provenance found for this line');
        }
    }), vscode.commands.registerCommand('diachron.timeline', () => {
        // Open timeline webview
        vscode.commands.executeCommand('diachron.timeline.focus');
    }), vscode.commands.registerCommand('diachron.status', async () => {
        const status = await client.status();
        if (status.connected) {
            vscode.window.showInformationMessage(`Diachron daemon running (v${status.version}). ` +
                `Events: ${status.eventCount}, DB: ${status.dbSize}`);
        }
        else {
            vscode.window.showWarningMessage('Diachron daemon not connected');
        }
    }), vscode.commands.registerCommand('diachron.startDaemon', () => {
        const terminal = vscode.window.createTerminal('Diachron');
        terminal.sendText('diachron daemon start');
        terminal.show();
    }), vscode.commands.registerCommand('diachron.viewSession', (sessionId) => {
        if (!sessionId) {
            vscode.window.showErrorMessage('No session ID provided');
            return;
        }
        // Open timeline filtered to this session
        const terminal = vscode.window.createTerminal('Diachron Session');
        terminal.sendText(`diachron timeline --session ${sessionId}`);
        terminal.show();
    }));
    // Listen for document changes to refresh decorations
    context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor(editor => {
        if (editor) {
            decorationProvider.refresh(editor);
        }
    }), vscode.workspace.onDidChangeTextDocument(event => {
        const editor = vscode.window.activeTextEditor;
        if (editor && event.document === editor.document) {
            decorationProvider.refresh(editor);
        }
    }));
    // Initial decoration for current editor
    if (vscode.window.activeTextEditor) {
        decorationProvider.refresh(vscode.window.activeTextEditor);
    }
    console.log('Diachron extension activated');
}
function deactivate() {
    if (client) {
        client.disconnect();
    }
}
//# sourceMappingURL=extension.js.map