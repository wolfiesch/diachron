import * as vscode from 'vscode';
import { DiachronClient, BlameMatch } from './diachronClient';

/**
 * VS Code HoverProvider that shows AI provenance information when hovering over code.
 *
 * Uses the Diachron daemon to look up which AI session wrote the code at the
 * current cursor position, including intent and confidence.
 */
export class DiachronHoverProvider implements vscode.HoverProvider {
    private client: DiachronClient;
    private cache: Map<string, { result: BlameMatch | null; timestamp: number }>;
    private cacheTTL: number;

    constructor(client: DiachronClient, options?: { cacheTTL?: number }) {
        this.client = client;
        this.cache = new Map();
        this.cacheTTL = options?.cacheTTL ?? 30000; // 30 second cache
    }

    async provideHover(
        document: vscode.TextDocument,
        position: vscode.Position,
        _token: vscode.CancellationToken
    ): Promise<vscode.Hover | null> {
        // Only process file scheme documents
        if (document.uri.scheme !== 'file') {
            return null;
        }

        const filePath = document.uri.fsPath;
        const lineNumber = position.line + 1; // Convert to 1-indexed
        const lineContent = document.lineAt(position.line).text;

        // Skip empty lines or whitespace-only lines
        if (!lineContent.trim()) {
            return null;
        }

        // Get context (±5 lines)
        const startLine = Math.max(0, position.line - 5);
        const endLine = Math.min(document.lineCount - 1, position.line + 5);
        const contextLines: string[] = [];
        for (let i = startLine; i <= endLine; i++) {
            contextLines.push(document.lineAt(i).text);
        }
        const context = contextLines.join('\n');

        // Check cache
        const cacheKey = `${filePath}:${lineNumber}:${lineContent}`;
        const cached = this.cache.get(cacheKey);
        if (cached && Date.now() - cached.timestamp < this.cacheTTL) {
            return cached.result ? this.formatHover(cached.result) : null;
        }

        // Query daemon for blame
        try {
            const blame = await this.client.blame(filePath, lineNumber, lineContent, context);

            // Cache result
            this.cache.set(cacheKey, { result: blame, timestamp: Date.now() });

            if (blame) {
                return this.formatHover(blame);
            }
        } catch (err) {
            console.error('Diachron hover error:', err);
        }

        return null;
    }

    /**
     * Format blame result as a rich Markdown hover card
     */
    private formatHover(blame: BlameMatch): vscode.Hover {
        const md = new vscode.MarkdownString();
        md.isTrusted = true;
        md.supportHtml = true;

        // Header with tool icon
        const toolIcon = this.getToolIcon(blame.event.tool_name);
        md.appendMarkdown(`### ${toolIcon} ${blame.event.tool_name}\n\n`);

        // Session and time info
        const sessionId = blame.event.session_id?.substring(0, 8) ?? 'unknown';
        const timeAgo = this.formatTimeAgo(blame.event.timestamp);
        md.appendMarkdown(`**Session:** \`${sessionId}\` • ${timeAgo}\n\n`);

        // Intent (if available)
        if (blame.intent) {
            md.appendMarkdown(`---\n\n`);
            md.appendMarkdown(`💬 *"${blame.intent}"*\n\n`);
        }

        // Confidence indicator
        md.appendMarkdown(`---\n\n`);
        const confidenceEmoji = this.getConfidenceEmoji(blame.confidence);
        md.appendMarkdown(`${confidenceEmoji} **${blame.confidence}** confidence\n\n`);

        // Match details
        if (blame.match_type) {
            md.appendMarkdown(`- Match type: ${blame.match_type}\n`);
        }
        if (blame.similarity > 0) {
            md.appendMarkdown(`- Similarity: ${(blame.similarity * 100).toFixed(0)}%\n`);
        }

        // Git context
        if (blame.event.git_branch || blame.event.git_commit_sha) {
            md.appendMarkdown(`\n---\n\n`);
            if (blame.event.git_branch) {
                md.appendMarkdown(`🌿 Branch: \`${blame.event.git_branch}\`\n`);
            }
            if (blame.event.git_commit_sha) {
                const shortSha = blame.event.git_commit_sha.substring(0, 7);
                md.appendMarkdown(`📦 Commit: \`${shortSha}\`\n`);
            }
        }

        // Operation details
        if (blame.event.operation || blame.event.diff_summary) {
            md.appendMarkdown(`\n---\n\n`);
            if (blame.event.operation) {
                md.appendMarkdown(`**Operation:** ${blame.event.operation}\n`);
            }
            if (blame.event.diff_summary) {
                md.appendMarkdown(`**Changes:** ${blame.event.diff_summary}\n`);
            }
        }

        // Action links (commands)
        md.appendMarkdown(`\n---\n\n`);
        md.appendMarkdown(
            `[View Session](command:diachron.viewSession?${encodeURIComponent(JSON.stringify(blame.event.session_id))}) • ` +
            `[Timeline](command:diachron.timeline)`
        );

        return new vscode.Hover(md);
    }

    /**
     * Get emoji icon for AI tool
     */
    private getToolIcon(toolName: string): string {
        const icons: Record<string, string> = {
            'Claude Code': '🤖',
            'Codex': '🔷',
            'Aider': '🛠️',
            'Cursor': '📝',
            'Write': '✏️',
            'Edit': '✏️',
            'Bash': '🖥️',
        };
        return icons[toolName] ?? '🤖';
    }

    /**
     * Get confidence level emoji
     */
    private getConfidenceEmoji(confidence: string): string {
        switch (confidence.toUpperCase()) {
            case 'HIGH':
                return '📊';
            case 'MEDIUM':
                return '📉';
            case 'LOW':
                return '❓';
            case 'INFERRED':
                return '🔮';
            default:
                return '📊';
        }
    }

    /**
     * Format timestamp as relative time
     */
    private formatTimeAgo(timestamp: string): string {
        try {
            const date = new Date(timestamp);
            const now = new Date();
            const diffMs = now.getTime() - date.getTime();
            const diffSec = Math.floor(diffMs / 1000);
            const diffMin = Math.floor(diffSec / 60);
            const diffHour = Math.floor(diffMin / 60);
            const diffDay = Math.floor(diffHour / 24);

            if (diffDay > 0) {
                return diffDay === 1 ? 'yesterday' : `${diffDay} days ago`;
            }
            if (diffHour > 0) {
                return diffHour === 1 ? '1 hour ago' : `${diffHour} hours ago`;
            }
            if (diffMin > 0) {
                return diffMin === 1 ? '1 minute ago' : `${diffMin} minutes ago`;
            }
            return 'just now';
        } catch {
            return timestamp;
        }
    }

    /**
     * Clear the cache (call when document changes)
     */
    clearCache(filePath?: string): void {
        if (filePath) {
            // Clear entries for specific file
            for (const key of this.cache.keys()) {
                if (key.startsWith(filePath + ':')) {
                    this.cache.delete(key);
                }
            }
        } else {
            this.cache.clear();
        }
    }
}
