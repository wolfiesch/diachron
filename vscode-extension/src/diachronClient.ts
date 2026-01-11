import * as net from 'net';
import * as path from 'path';
import * as os from 'os';

/**
 * Response types from the Diachron daemon
 */
export interface StatusResponse {
    connected: boolean;
    version?: string;
    eventCount?: number;
    dbSize?: string;
}

export interface BlameMatch {
    event: {
        id: number;
        timestamp: string;
        tool_name: string;
        file_path: string;
        operation: string;
        diff_summary?: string;
        session_id?: string;
        git_branch?: string;
        git_commit_sha?: string;
    };
    confidence: string;
    match_type: string;
    similarity: number;
    intent?: string;
}

export interface BlameResult {
    match?: BlameMatch;
    error?: string;
}

/**
 * Client for communicating with the Diachron daemon via Unix socket IPC.
 *
 * The daemon listens on ~/.diachron/diachron.sock and accepts JSON messages
 * with newline delimiters.
 */
export class DiachronClient {
    private socketPath: string;
    private timeout: number;

    constructor(options?: { socketPath?: string; timeout?: number }) {
        this.socketPath = options?.socketPath ?? this.getDefaultSocketPath();
        this.timeout = options?.timeout ?? 5000;
    }

    /**
     * Get the default socket path based on platform
     */
    private getDefaultSocketPath(): string {
        const homeDir = os.homedir();
        return path.join(homeDir, '.diachron', 'diachron.sock');
    }

    /**
     * Send a message to the daemon and receive a response
     */
    private async sendMessage<T>(message: object): Promise<T> {
        return new Promise((resolve, reject) => {
            const client = net.createConnection({ path: this.socketPath }, () => {
                // Send JSON message with newline delimiter
                const data = JSON.stringify(message) + '\n';
                client.write(data);
            });

            let responseData = '';

            client.setTimeout(this.timeout);

            client.on('data', (chunk) => {
                responseData += chunk.toString();

                // Check if we have a complete JSON message (ends with newline)
                if (responseData.includes('\n')) {
                    try {
                        const response = JSON.parse(responseData.trim());
                        client.end();
                        resolve(response as T);
                    } catch (e) {
                        // Wait for more data if JSON is incomplete
                    }
                }
            });

            client.on('timeout', () => {
                client.destroy();
                reject(new Error('Connection timed out'));
            });

            client.on('error', (err) => {
                reject(err);
            });

            client.on('end', () => {
                if (responseData) {
                    try {
                        const response = JSON.parse(responseData.trim());
                        resolve(response as T);
                    } catch (e) {
                        reject(new Error(`Invalid JSON response: ${responseData}`));
                    }
                }
            });
        });
    }

    /**
     * Check daemon status and connectivity using Ping message
     */
    async status(): Promise<StatusResponse> {
        try {
            const response = await this.sendMessage<{
                type: string;
                payload?: {
                    uptime_secs: number;
                    events_count: number;
                };
            }>({ type: 'Ping' });

            if (response.type === 'Pong' && response.payload) {
                return {
                    connected: true,
                    version: 'running',
                    eventCount: response.payload.events_count,
                    dbSize: `${Math.floor(response.payload.uptime_secs / 60)}m uptime`,
                };
            }

            return { connected: true };
        } catch (err) {
            return { connected: false };
        }
    }

    /**
     * Get blame information for a specific file and line
     */
    async blame(
        filePath: string,
        lineNumber: number,
        content: string,
        context?: string
    ): Promise<BlameMatch | null> {
        try {
            const response = await this.sendMessage<{
                type: string;
                payload?: BlameMatch | { reason: string };
            }>({
                type: 'BlameByFingerprint',
                payload: {
                    file_path: filePath,
                    line_number: lineNumber,
                    content: content,
                    context: context ?? '',
                    mode: 'best-effort',
                },
            });

            if (response.type === 'BlameResult' && response.payload) {
                return response.payload as BlameMatch;
            }

            return null;
        } catch (err) {
            console.error('Blame lookup failed:', err);
            return null;
        }
    }

    /**
     * Get timeline events for a file path (for decoration/gutter icons)
     */
    async getFileEvents(filePath: string): Promise<BlameMatch[]> {
        try {
            // Use Timeline message to get events filtered by file path
            const response = await this.sendMessage<{
                type: string;
                payload?: Array<{
                    id: number;
                    timestamp: string;
                    tool_name: string;
                    file_path: string;
                    operation: string;
                    diff_summary?: string;
                    session_id?: string;
                    git_branch?: string;
                    git_commit_sha?: string;
                }>;
            }>({
                type: 'Timeline',
                payload: {
                    since: null,
                    file_filter: filePath,
                    limit: 100,
                },
            });

            if (response.type === 'Events' && response.payload) {
                // Convert StoredEvent to BlameMatch format
                return response.payload.map(event => ({
                    event,
                    confidence: 'medium',
                    match_type: 'timeline',
                    similarity: 1.0,
                    intent: undefined,
                }));
            }

            return [];
        } catch (err) {
            console.error('File events lookup failed:', err);
            return [];
        }
    }

    /**
     * Disconnect from the daemon (cleanup)
     */
    disconnect(): void {
        // No persistent connection to clean up - each request creates a new connection
    }

    /**
     * Format bytes to human-readable string
     */
    private formatBytes(bytes: number): string {
        if (bytes === 0) return '0 B';
        const k = 1024;
        const sizes = ['B', 'KB', 'MB', 'GB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
    }
}
