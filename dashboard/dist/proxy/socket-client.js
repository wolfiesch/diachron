import * as net from 'net';
import * as os from 'os';
import * as path from 'path';
const SOCKET_PATH = path.join(os.homedir(), '.diachron', 'diachron.sock');
const CONNECT_TIMEOUT = 5000;
const REQUEST_TIMEOUT = 30000;
/**
 * Send a message to the Diachron daemon via Unix socket
 * and return the response.
 */
export async function sendToDaemon(message) {
    return new Promise((resolve, reject) => {
        const socket = new net.Socket();
        let responseData = '';
        let connected = false;
        const connectTimeout = setTimeout(() => {
            if (!connected) {
                socket.destroy();
                reject(new Error('Connection timeout: daemon not responding'));
            }
        }, CONNECT_TIMEOUT);
        const requestTimeout = setTimeout(() => {
            socket.destroy();
            reject(new Error('Request timeout: daemon took too long to respond'));
        }, REQUEST_TIMEOUT);
        socket.on('connect', () => {
            connected = true;
            clearTimeout(connectTimeout);
            // Send the message as JSON with newline delimiter
            const jsonMessage = JSON.stringify(message) + '\n';
            socket.write(jsonMessage);
        });
        socket.on('data', (data) => {
            responseData += data.toString();
            // Check for complete JSON response (newline-delimited)
            if (responseData.includes('\n')) {
                try {
                    const response = JSON.parse(responseData.trim());
                    clearTimeout(requestTimeout);
                    socket.destroy();
                    resolve(response);
                }
                catch {
                    // Incomplete JSON, wait for more data
                }
            }
        });
        socket.on('error', (err) => {
            clearTimeout(connectTimeout);
            clearTimeout(requestTimeout);
            if (err.code === 'ENOENT') {
                reject(new Error('Daemon not running: socket not found at ' + SOCKET_PATH));
            }
            else if (err.code === 'ECONNREFUSED') {
                reject(new Error('Daemon not responding: connection refused'));
            }
            else {
                reject(err);
            }
        });
        socket.on('close', () => {
            clearTimeout(connectTimeout);
            clearTimeout(requestTimeout);
        });
        // Connect to the Unix socket
        socket.connect(SOCKET_PATH);
    });
}
/**
 * Check if daemon is running and responsive
 */
export async function pingDaemon() {
    const response = await sendToDaemon({ type: 'Ping' });
    if (response.type === 'Pong' && response.payload) {
        return response.payload;
    }
    throw new Error('Unexpected response from daemon');
}
/**
 * Get diagnostic info from daemon
 */
export async function getDiagnostics() {
    const response = await sendToDaemon({ type: 'DoctorInfo' });
    if (response.type === 'Doctor' && response.payload) {
        return response.payload;
    }
    throw new Error('Failed to get diagnostics');
}
/**
 * Query timeline events
 */
export async function queryTimeline(options) {
    const response = await sendToDaemon({
        type: 'Timeline',
        payload: {
            since: options.since || null,
            file_filter: options.file_filter || null,
            limit: options.limit || 100,
        },
    });
    if (response.type === 'Events' && response.payload) {
        return response.payload;
    }
    throw new Error('Failed to query timeline');
}
/**
 * Search events and exchanges
 */
export async function search(options) {
    const response = await sendToDaemon({
        type: 'Search',
        payload: {
            query: options.query,
            limit: options.limit || 20,
            source_filter: options.source_filter || null,
            since: options.since || null,
            project: options.project || null,
        },
    });
    if (response.type === 'SearchResults' && response.payload) {
        return response.payload;
    }
    throw new Error('Failed to search');
}
/**
 * Blame a specific line in a file
 */
export async function blameByFingerprint(options) {
    const response = await sendToDaemon({
        type: 'BlameByFingerprint',
        payload: {
            file_path: options.file_path,
            line_number: options.line_number,
            content: options.content,
            context: options.context,
            mode: options.mode || 'best-effort',
        },
    });
    if (response.type === 'BlameResult' && response.payload) {
        return response.payload;
    }
    if (response.type === 'BlameNotFound') {
        return null;
    }
    throw new Error('Failed to blame');
}
/**
 * Generate evidence pack for a PR
 */
export async function correlateEvidence(options) {
    const response = await sendToDaemon({
        type: 'CorrelateEvidence',
        payload: {
            pr_id: options.pr_id,
            commits: options.commits,
            branch: options.branch,
            start_time: options.start_time,
            end_time: options.end_time,
            intent: options.intent || null,
        },
    });
    if (response.type === 'EvidenceResult' && response.payload) {
        return response.payload;
    }
    throw new Error('Failed to correlate evidence');
}
/**
 * Run database maintenance
 */
export async function runMaintenance(retentionDays = 0) {
    const response = await sendToDaemon({
        type: 'Maintenance',
        payload: { retention_days: retentionDays },
    });
    if (response.type === 'MaintenanceStats' && response.payload) {
        return response.payload;
    }
    throw new Error('Failed to run maintenance');
}
