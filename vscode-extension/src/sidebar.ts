import * as vscode from 'vscode';
import * as path from 'path';
import { DiachronClient, BlameMatch } from './diachronClient';

/**
 * Tree item representing an event in the timeline
 */
export class TimelineItem extends vscode.TreeItem {
    constructor(
        public readonly event: BlameMatch,
        public readonly collapsibleState: vscode.TreeItemCollapsibleState
    ) {
        super(TimelineItem.getLabel(event), collapsibleState);

        this.tooltip = this.getTooltip();
        this.description = this.getDescription();
        this.iconPath = this.getIcon();
        this.contextValue = 'timelineEvent';

        // Add command to navigate to file when clicked
        if (event.event.file_path) {
            this.command = {
                command: 'diachron.openEvent',
                title: 'Open Event',
                arguments: [event]
            };
        }
    }

    private static getLabel(event: BlameMatch): string {
        const fileName = event.event.file_path
            ? path.basename(event.event.file_path)
            : 'Unknown file';
        return `${event.event.tool_name}: ${fileName}`;
    }

    private getDescription(): string {
        return this.formatTimeAgo(this.event.event.timestamp);
    }

    private getTooltip(): string {
        const parts = [
            `Tool: ${this.event.event.tool_name}`,
            `File: ${this.event.event.file_path || 'N/A'}`,
            `Operation: ${this.event.event.operation}`,
            `Time: ${this.event.event.timestamp}`,
        ];

        if (this.event.intent) {
            parts.push(`Intent: ${this.event.intent}`);
        }

        if (this.event.event.git_branch) {
            parts.push(`Branch: ${this.event.event.git_branch}`);
        }

        return parts.join('\n');
    }

    private getIcon(): vscode.ThemeIcon {
        const tool = this.event.event.tool_name.toLowerCase();
        if (tool.includes('write') || tool.includes('edit')) {
            return new vscode.ThemeIcon('edit');
        } else if (tool.includes('bash')) {
            return new vscode.ThemeIcon('terminal');
        } else if (tool.includes('codex')) {
            return new vscode.ThemeIcon('sparkle');
        } else {
            return new vscode.ThemeIcon('robot');
        }
    }

    private formatTimeAgo(timestamp: string): string {
        try {
            const date = new Date(timestamp);
            const now = new Date();
            const diffMs = now.getTime() - date.getTime();
            const diffMin = Math.floor(diffMs / 60000);
            const diffHour = Math.floor(diffMin / 60);
            const diffDay = Math.floor(diffHour / 24);

            if (diffDay > 0) {
                return diffDay === 1 ? '1 day ago' : `${diffDay} days ago`;
            }
            if (diffHour > 0) {
                return diffHour === 1 ? '1 hour ago' : `${diffHour} hours ago`;
            }
            if (diffMin > 0) {
                return diffMin === 1 ? '1 min ago' : `${diffMin} mins ago`;
            }
            return 'just now';
        } catch {
            return '';
        }
    }
}

/**
 * TreeDataProvider for the Timeline view in the sidebar.
 * Shows recent AI-generated events for the current workspace.
 */
export class TimelineTreeProvider implements vscode.TreeDataProvider<TimelineItem> {
    private _onDidChangeTreeData: vscode.EventEmitter<TimelineItem | undefined | null | void> =
        new vscode.EventEmitter<TimelineItem | undefined | null | void>();
    readonly onDidChangeTreeData: vscode.Event<TimelineItem | undefined | null | void> =
        this._onDidChangeTreeData.event;

    private client: DiachronClient;
    private events: BlameMatch[] = [];
    private loading = false;

    constructor(client: DiachronClient) {
        this.client = client;
    }

    /**
     * Refresh the timeline data
     */
    refresh(): void {
        this._onDidChangeTreeData.fire();
    }

    /**
     * Load events from the daemon
     */
    async loadEvents(): Promise<void> {
        if (this.loading) return;
        this.loading = true;

        try {
            // Get events for all files in the workspace
            const workspaceFolders = vscode.workspace.workspaceFolders;
            if (!workspaceFolders || workspaceFolders.length === 0) {
                this.events = [];
                return;
            }

            const workspacePath = workspaceFolders[0].uri.fsPath;
            this.events = await this.client.getFileEvents(workspacePath);

            // Sort by timestamp (newest first)
            this.events.sort((a, b) => {
                const timeA = new Date(a.event.timestamp).getTime();
                const timeB = new Date(b.event.timestamp).getTime();
                return timeB - timeA;
            });

            // Limit to most recent 50 events
            this.events = this.events.slice(0, 50);
        } catch (err) {
            console.error('Failed to load timeline events:', err);
            this.events = [];
        } finally {
            this.loading = false;
        }
    }

    getTreeItem(element: TimelineItem): vscode.TreeItem {
        return element;
    }

    async getChildren(element?: TimelineItem): Promise<TimelineItem[]> {
        if (element) {
            // No children for event items
            return [];
        }

        // Root level - load and return events
        await this.loadEvents();
        return this.events.map(
            event => new TimelineItem(event, vscode.TreeItemCollapsibleState.None)
        );
    }
}

/**
 * Tree item for blame detail properties
 */
export class BlameDetailItem extends vscode.TreeItem {
    constructor(
        public readonly label: string,
        public readonly value: string,
        public readonly contextValue: string = 'blameDetail'
    ) {
        super(label, vscode.TreeItemCollapsibleState.None);
        this.description = value;
    }
}

/**
 * TreeDataProvider for the Blame Details view in the sidebar.
 * Shows detailed information about the currently hovered/selected blame.
 */
export class BlameDetailsProvider implements vscode.TreeDataProvider<BlameDetailItem> {
    private _onDidChangeTreeData: vscode.EventEmitter<BlameDetailItem | undefined | null | void> =
        new vscode.EventEmitter<BlameDetailItem | undefined | null | void>();
    readonly onDidChangeTreeData: vscode.Event<BlameDetailItem | undefined | null | void> =
        this._onDidChangeTreeData.event;

    private currentBlame: BlameMatch | null = null;

    /**
     * Update the displayed blame information
     */
    setBlame(blame: BlameMatch | null): void {
        this.currentBlame = blame;
        this._onDidChangeTreeData.fire();
    }

    /**
     * Clear the displayed blame
     */
    clear(): void {
        this.currentBlame = null;
        this._onDidChangeTreeData.fire();
    }

    getTreeItem(element: BlameDetailItem): vscode.TreeItem {
        return element;
    }

    getChildren(element?: BlameDetailItem): BlameDetailItem[] {
        if (element || !this.currentBlame) {
            return [];
        }

        const blame = this.currentBlame;
        const items: BlameDetailItem[] = [];

        // Tool info
        items.push(new BlameDetailItem('Tool', blame.event.tool_name));
        items.push(new BlameDetailItem('Operation', blame.event.operation));

        // Session info
        if (blame.event.session_id) {
            items.push(new BlameDetailItem('Session', blame.event.session_id.substring(0, 12)));
        }

        // Time info
        items.push(new BlameDetailItem('Timestamp', blame.event.timestamp));

        // Intent
        if (blame.intent) {
            items.push(new BlameDetailItem('Intent', blame.intent));
        }

        // Confidence
        items.push(new BlameDetailItem('Confidence', blame.confidence));
        items.push(new BlameDetailItem('Match Type', blame.match_type));
        if (blame.similarity > 0) {
            items.push(new BlameDetailItem('Similarity', `${(blame.similarity * 100).toFixed(0)}%`));
        }

        // Git info
        if (blame.event.git_branch) {
            items.push(new BlameDetailItem('Branch', blame.event.git_branch));
        }
        if (blame.event.git_commit_sha) {
            items.push(new BlameDetailItem('Commit', blame.event.git_commit_sha.substring(0, 7)));
        }

        // Diff summary
        if (blame.event.diff_summary) {
            items.push(new BlameDetailItem('Changes', blame.event.diff_summary));
        }

        // File path
        if (blame.event.file_path) {
            items.push(new BlameDetailItem('File', blame.event.file_path));
        }

        return items;
    }
}

/**
 * Register sidebar providers and commands
 */
export function registerSidebarProviders(
    context: vscode.ExtensionContext,
    client: DiachronClient
): { timeline: TimelineTreeProvider; blameDetails: BlameDetailsProvider } {
    // Create providers
    const timelineProvider = new TimelineTreeProvider(client);
    const blameDetailsProvider = new BlameDetailsProvider();

    // Register tree data providers
    context.subscriptions.push(
        vscode.window.registerTreeDataProvider('diachron.timeline', timelineProvider),
        vscode.window.registerTreeDataProvider('diachron.blame', blameDetailsProvider)
    );

    // Register refresh command
    context.subscriptions.push(
        vscode.commands.registerCommand('diachron.refreshTimeline', () => {
            timelineProvider.refresh();
        })
    );

    // Register open event command
    context.subscriptions.push(
        vscode.commands.registerCommand('diachron.openEvent', async (event: BlameMatch) => {
            if (event.event.file_path) {
                try {
                    const doc = await vscode.workspace.openTextDocument(event.event.file_path);
                    await vscode.window.showTextDocument(doc);

                    // If we have line information, navigate to it
                    const lineMatch = event.event.diff_summary?.match(/lines?\s+(\d+)/i);
                    if (lineMatch) {
                        const line = parseInt(lineMatch[1], 10) - 1;
                        const editor = vscode.window.activeTextEditor;
                        if (editor && line >= 0 && line < editor.document.lineCount) {
                            const position = new vscode.Position(line, 0);
                            editor.selection = new vscode.Selection(position, position);
                            editor.revealRange(
                                new vscode.Range(position, position),
                                vscode.TextEditorRevealType.InCenter
                            );
                        }
                    }
                } catch (err) {
                    vscode.window.showErrorMessage(`Could not open file: ${event.event.file_path}`);
                }
            }
        })
    );

    // Auto-refresh timeline periodically
    const refreshInterval = setInterval(() => {
        timelineProvider.refresh();
    }, 60000); // Refresh every minute

    context.subscriptions.push({
        dispose: () => clearInterval(refreshInterval)
    });

    return { timeline: timelineProvider, blameDetails: blameDetailsProvider };
}
