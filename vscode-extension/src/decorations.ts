import * as vscode from 'vscode';
import * as path from 'path';
import { DiachronClient, BlameMatch } from './diachronClient';

/**
 * Decoration provider for Diachron gutter icons.
 *
 * Shows visual indicators in the editor gutter for lines that were written
 * by AI assistants. Different colors indicate different confidence levels.
 */
export class DiachronDecorationProvider implements vscode.Disposable {
    private client: DiachronClient;
    private disposables: vscode.Disposable[] = [];

    // Decoration types for different confidence levels
    private highConfidenceDecoration: vscode.TextEditorDecorationType;
    private mediumConfidenceDecoration: vscode.TextEditorDecorationType;
    private lowConfidenceDecoration: vscode.TextEditorDecorationType;

    // Cache for file decorations
    private decorationCache: Map<string, {
        decorations: Map<number, BlameMatch>;
        timestamp: number;
    }>;
    private cacheTTL: number;

    // Debounce timer
    private refreshTimer: NodeJS.Timeout | null = null;
    private refreshDelay: number = 500;

    constructor(client: DiachronClient, options?: { cacheTTL?: number }) {
        this.client = client;
        this.decorationCache = new Map();
        this.cacheTTL = options?.cacheTTL ?? 60000; // 1 minute cache

        // Create decoration types with different colors for confidence levels
        this.highConfidenceDecoration = vscode.window.createTextEditorDecorationType({
            gutterIconPath: this.getIconPath('ai-high.svg'),
            gutterIconSize: 'contain',
            overviewRulerColor: '#4CAF50',
            overviewRulerLane: vscode.OverviewRulerLane.Left,
        });

        this.mediumConfidenceDecoration = vscode.window.createTextEditorDecorationType({
            gutterIconPath: this.getIconPath('ai-medium.svg'),
            gutterIconSize: 'contain',
            overviewRulerColor: '#FFC107',
            overviewRulerLane: vscode.OverviewRulerLane.Left,
        });

        this.lowConfidenceDecoration = vscode.window.createTextEditorDecorationType({
            gutterIconPath: this.getIconPath('ai-low.svg'),
            gutterIconSize: 'contain',
            overviewRulerColor: '#9E9E9E',
            overviewRulerLane: vscode.OverviewRulerLane.Left,
        });
    }

    /**
     * Get path to gutter icon SVG
     */
    private getIconPath(iconName: string): string {
        // Icons will be in the extension's resources folder
        return path.join(__dirname, '..', 'resources', 'icons', iconName);
    }

    /**
     * Refresh decorations for the given editor
     */
    async refresh(editor: vscode.TextEditor): Promise<void> {
        // Debounce rapid refresh calls
        if (this.refreshTimer) {
            clearTimeout(this.refreshTimer);
        }

        this.refreshTimer = setTimeout(async () => {
            await this.updateDecorations(editor);
        }, this.refreshDelay);
    }

    /**
     * Actually update the decorations (called after debounce)
     */
    private async updateDecorations(editor: vscode.TextEditor): Promise<void> {
        const document = editor.document;

        // Only process file scheme documents
        if (document.uri.scheme !== 'file') {
            return;
        }

        const filePath = document.uri.fsPath;

        // Check if decorations are enabled in settings
        const config = vscode.workspace.getConfiguration('diachron');
        if (!config.get<boolean>('showGutterIcons', true)) {
            this.clearDecorations(editor);
            return;
        }

        // Check cache
        const cached = this.decorationCache.get(filePath);
        if (cached && Date.now() - cached.timestamp < this.cacheTTL) {
            this.applyDecorations(editor, cached.decorations);
            return;
        }

        // Query daemon for all events in this file
        try {
            const events = await this.client.getFileEvents(filePath);

            // Build line -> blame map
            const lineBlame = new Map<number, BlameMatch>();

            for (const event of events) {
                // Parse line numbers from diff_summary or operation
                const lines = this.extractLineNumbers(event);
                for (const line of lines) {
                    // Keep the most confident match for each line
                    const existing = lineBlame.get(line);
                    if (!existing || this.isHigherConfidence(event.confidence, existing.confidence)) {
                        lineBlame.set(line, event);
                    }
                }
            }

            // Cache the result
            this.decorationCache.set(filePath, {
                decorations: lineBlame,
                timestamp: Date.now(),
            });

            // Apply decorations
            this.applyDecorations(editor, lineBlame);
        } catch (err) {
            console.error('Diachron decoration error:', err);
        }
    }

    /**
     * Apply decorations to the editor based on blame data
     */
    private applyDecorations(
        editor: vscode.TextEditor,
        lineBlame: Map<number, BlameMatch>
    ): void {
        const highRanges: vscode.Range[] = [];
        const mediumRanges: vscode.Range[] = [];
        const lowRanges: vscode.Range[] = [];

        for (const [lineNumber, blame] of lineBlame) {
            // Convert 1-indexed to 0-indexed
            const line = lineNumber - 1;

            // Skip if line is out of range
            if (line < 0 || line >= editor.document.lineCount) {
                continue;
            }

            const range = new vscode.Range(line, 0, line, 0);

            // Sort by confidence
            switch (blame.confidence.toUpperCase()) {
                case 'HIGH':
                    highRanges.push(range);
                    break;
                case 'MEDIUM':
                    mediumRanges.push(range);
                    break;
                default:
                    lowRanges.push(range);
                    break;
            }
        }

        // Apply decorations
        editor.setDecorations(this.highConfidenceDecoration, highRanges);
        editor.setDecorations(this.mediumConfidenceDecoration, mediumRanges);
        editor.setDecorations(this.lowConfidenceDecoration, lowRanges);
    }

    /**
     * Clear all decorations from an editor
     */
    private clearDecorations(editor: vscode.TextEditor): void {
        editor.setDecorations(this.highConfidenceDecoration, []);
        editor.setDecorations(this.mediumConfidenceDecoration, []);
        editor.setDecorations(this.lowConfidenceDecoration, []);
    }

    /**
     * Extract line numbers from a blame event
     */
    private extractLineNumbers(blame: BlameMatch): number[] {
        const lines: number[] = [];

        // Try to parse from diff_summary
        // Format might be: "+12 lines at 38-50" or "modify lines 10-20"
        if (blame.event.diff_summary) {
            const rangeMatch = blame.event.diff_summary.match(/(?:lines?\s+)?(\d+)(?:-(\d+))?/i);
            if (rangeMatch) {
                const start = parseInt(rangeMatch[1], 10);
                const end = rangeMatch[2] ? parseInt(rangeMatch[2], 10) : start;
                for (let i = start; i <= end; i++) {
                    lines.push(i);
                }
            }
        }

        return lines;
    }

    /**
     * Compare confidence levels
     */
    private isHigherConfidence(a: string, b: string): boolean {
        const order: Record<string, number> = {
            'HIGH': 3,
            'MEDIUM': 2,
            'LOW': 1,
            'INFERRED': 0,
        };
        return (order[a.toUpperCase()] ?? 0) > (order[b.toUpperCase()] ?? 0);
    }

    /**
     * Invalidate cache for a file
     */
    invalidateCache(filePath?: string): void {
        if (filePath) {
            this.decorationCache.delete(filePath);
        } else {
            this.decorationCache.clear();
        }
    }

    /**
     * Dispose of resources
     */
    dispose(): void {
        if (this.refreshTimer) {
            clearTimeout(this.refreshTimer);
        }

        this.highConfidenceDecoration.dispose();
        this.mediumConfidenceDecoration.dispose();
        this.lowConfidenceDecoration.dispose();

        this.disposables.forEach(d => d.dispose());
    }
}
