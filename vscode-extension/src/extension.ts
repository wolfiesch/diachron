import * as vscode from 'vscode';
import { DiachronClient } from './diachronClient';
import { DiachronHoverProvider } from './hoverProvider';
import { DiachronDecorationProvider } from './decorations';
import { registerSidebarProviders, BlameDetailsProvider } from './sidebar';

let client: DiachronClient;
let decorationProvider: DiachronDecorationProvider;
let blameDetailsProvider: BlameDetailsProvider;

export async function activate(context: vscode.ExtensionContext) {
    console.log('Diachron extension activating...');

    // Initialize daemon client
    client = new DiachronClient();

    // Check daemon status
    const status = await client.status();
    if (!status.connected) {
        vscode.window.showWarningMessage(
            'Diachron daemon not running. Run `diachron daemon start` to enable AI blame.',
            'Start Daemon'
        ).then(selection => {
            if (selection === 'Start Daemon') {
                vscode.commands.executeCommand('diachron.startDaemon');
            }
        });
    } else {
        console.log(`Diachron connected: ${status.version}`);
    }

    // Register hover provider for all languages
    const hoverProvider = new DiachronHoverProvider(client);
    context.subscriptions.push(
        vscode.languages.registerHoverProvider({ scheme: 'file' }, hoverProvider)
    );

    // Register decoration provider for gutter icons
    decorationProvider = new DiachronDecorationProvider(client);
    context.subscriptions.push(decorationProvider);

    // Register sidebar providers (timeline + blame details)
    const sidebarProviders = registerSidebarProviders(context, client);
    blameDetailsProvider = sidebarProviders.blameDetails;

    // Register commands
    context.subscriptions.push(
        vscode.commands.registerCommand('diachron.blame', async () => {
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
                vscode.window.showInformationMessage(
                    `${blame.event.tool_name} (${blame.confidence}): ${blame.intent || 'No intent recorded'}`
                );
            } else {
                blameDetailsProvider.clear();
                vscode.window.showInformationMessage('No AI provenance found for this line');
            }
        }),

        vscode.commands.registerCommand('diachron.timeline', () => {
            // Open timeline webview
            vscode.commands.executeCommand('diachron.timeline.focus');
        }),

        vscode.commands.registerCommand('diachron.status', async () => {
            const status = await client.status();
            if (status.connected) {
                vscode.window.showInformationMessage(
                    `Diachron daemon running (v${status.version}). ` +
                    `Events: ${status.eventCount}, DB: ${status.dbSize}`
                );
            } else {
                vscode.window.showWarningMessage('Diachron daemon not connected');
            }
        }),

        vscode.commands.registerCommand('diachron.startDaemon', () => {
            const terminal = vscode.window.createTerminal('Diachron');
            terminal.sendText('diachron daemon start');
            terminal.show();
        }),

        vscode.commands.registerCommand('diachron.viewSession', (sessionId: string) => {
            if (!sessionId) {
                vscode.window.showErrorMessage('No session ID provided');
                return;
            }
            // Open timeline filtered to this session
            const terminal = vscode.window.createTerminal('Diachron Session');
            terminal.sendText(`diachron timeline --session ${sessionId}`);
            terminal.show();
        })
    );

    // Listen for document changes to refresh decorations
    context.subscriptions.push(
        vscode.window.onDidChangeActiveTextEditor(editor => {
            if (editor) {
                decorationProvider.refresh(editor);
            }
        }),
        vscode.workspace.onDidChangeTextDocument(event => {
            const editor = vscode.window.activeTextEditor;
            if (editor && event.document === editor.document) {
                decorationProvider.refresh(editor);
            }
        })
    );

    // Initial decoration for current editor
    if (vscode.window.activeTextEditor) {
        decorationProvider.refresh(vscode.window.activeTextEditor);
    }

    console.log('Diachron extension activated');
}

export function deactivate() {
    if (client) {
        client.disconnect();
    }
}
