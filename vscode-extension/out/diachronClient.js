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
exports.DiachronClient = void 0;
const net = __importStar(require("net"));
const path = __importStar(require("path"));
const os = __importStar(require("os"));
/**
 * Client for communicating with the Diachron daemon via Unix socket IPC.
 *
 * The daemon listens on ~/.diachron/diachron.sock and accepts JSON messages
 * with newline delimiters.
 */
class DiachronClient {
    socketPath;
    timeout;
    constructor(options) {
        this.socketPath = options?.socketPath ?? this.getDefaultSocketPath();
        this.timeout = options?.timeout ?? 5000;
    }
    /**
     * Get the default socket path based on platform
     */
    getDefaultSocketPath() {
        const homeDir = os.homedir();
        return path.join(homeDir, '.diachron', 'diachron.sock');
    }
    /**
     * Send a message to the daemon and receive a response
     */
    async sendMessage(message) {
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
                        resolve(response);
                    }
                    catch (e) {
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
                        resolve(response);
                    }
                    catch (e) {
                        reject(new Error(`Invalid JSON response: ${responseData}`));
                    }
                }
            });
        });
    }
    /**
     * Check daemon status and connectivity using Ping message
     */
    async status() {
        try {
            const response = await this.sendMessage({ type: 'Ping' });
            if (response.type === 'Pong' && response.payload) {
                return {
                    connected: true,
                    version: 'running',
                    eventCount: response.payload.events_count,
                    dbSize: `${Math.floor(response.payload.uptime_secs / 60)}m uptime`,
                };
            }
            return { connected: true };
        }
        catch (err) {
            return { connected: false };
        }
    }
    /**
     * Get blame information for a specific file and line
     */
    async blame(filePath, lineNumber, content, context) {
        try {
            const response = await this.sendMessage({
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
                return response.payload;
            }
            return null;
        }
        catch (err) {
            console.error('Blame lookup failed:', err);
            return null;
        }
    }
    /**
     * Get timeline events for a file path (for decoration/gutter icons)
     */
    async getFileEvents(filePath) {
        try {
            // Use Timeline message to get events filtered by file path
            const response = await this.sendMessage({
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
        }
        catch (err) {
            console.error('File events lookup failed:', err);
            return [];
        }
    }
    /**
     * Disconnect from the daemon (cleanup)
     */
    disconnect() {
        // No persistent connection to clean up - each request creates a new connection
    }
    /**
     * Format bytes to human-readable string
     */
    formatBytes(bytes) {
        if (bytes === 0)
            return '0 B';
        const k = 1024;
        const sizes = ['B', 'KB', 'MB', 'GB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
    }
}
exports.DiachronClient = DiachronClient;
//# sourceMappingURL=diachronClient.js.map