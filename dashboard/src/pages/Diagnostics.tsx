import { motion } from 'framer-motion';
import {
  Stethoscope,
  Activity,
  Database,
  Cpu,
  HardDrive,
  Sparkles,
  CheckCircle,
  XCircle,
  RefreshCw,
  Trash2,
} from 'lucide-react';
import { useDiagnostics, useHealth, useMaintenance } from '@/api/hooks';
import { formatBytes, formatDuration } from '@/types/diachron';
import { cn } from '@/lib/utils';
import { useState } from 'react';

export function DiagnosticsPage() {
  const { data: health, isLoading: healthLoading, refetch: refetchHealth } = useHealth();
  const { data: diagnostics, isLoading: diagnosticsLoading, refetch: refetchDiagnostics } = useDiagnostics();
  const { mutate: runMaintenance, isPending: maintenancePending } = useMaintenance();

  const [retentionDays, setRetentionDays] = useState(90);
  const [maintenanceResult, setMaintenanceResult] = useState<{
    size_before: number;
    size_after: number;
    events_pruned: number;
    exchanges_pruned: number;
    duration_ms: number;
  } | null>(null);

  const isConnected = health?.status === 'ok';
  const isLoading = healthLoading || diagnosticsLoading;

  const handleMaintenance = () => {
    runMaintenance(retentionDays, {
      onSuccess: (result) => {
        setMaintenanceResult(result);
        refetchDiagnostics();
      },
    });
  };

  const handleRefresh = () => {
    refetchHealth();
    refetchDiagnostics();
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <motion.div
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
        className="flex items-center justify-between"
      >
        <div>
          <h1 className="text-3xl font-display font-bold text-noir-100">
            Diagnostics
          </h1>
          <p className="text-noir-400 mt-1">
            System health and maintenance
          </p>
        </div>
        <button
          onClick={handleRefresh}
          disabled={isLoading}
          className="btn btn-secondary"
        >
          <RefreshCw size={16} className={cn(isLoading && 'animate-spin')} />
          Refresh
        </button>
      </motion.div>

      {/* Status Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Daemon Status */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          className="stat-card"
        >
          <div className="flex items-start justify-between">
            <div>
              <p className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                Daemon Status
              </p>
              <div className="flex items-center gap-2 mt-2">
                {isConnected ? (
                  <>
                    <CheckCircle size={20} className="text-confidence-high" />
                    <span className="text-lg font-semibold text-confidence-high">Connected</span>
                  </>
                ) : (
                  <>
                    <XCircle size={20} className="text-op-delete" />
                    <span className="text-lg font-semibold text-op-delete">Disconnected</span>
                  </>
                )}
              </div>
              {isConnected && diagnostics && (
                <p className="text-xs text-noir-500 mt-1">
                  Uptime: {formatDuration(diagnostics.uptime_secs)}
                </p>
              )}
            </div>
            <div className="p-3 rounded-lg bg-noir-800/50">
              <Activity size={24} className={isConnected ? 'text-confidence-high' : 'text-op-delete'} />
            </div>
          </div>
        </motion.div>

        {/* Events Count */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.1 }}
          className="stat-card"
        >
          <div className="flex items-start justify-between">
            <div>
              <p className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                Events Tracked
              </p>
              <p className="text-3xl font-display font-semibold text-noir-100 mt-1 tabular-nums">
                {diagnostics?.events_count.toLocaleString() || '—'}
              </p>
              {diagnostics && (
                <p className="text-xs text-noir-500 mt-1">
                  {diagnostics.exchanges_count.toLocaleString()} exchanges indexed
                </p>
              )}
            </div>
            <div className="p-3 rounded-lg bg-noir-800/50">
              <Database size={24} className="text-accent-primary" />
            </div>
          </div>
        </motion.div>

        {/* Database Size */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2 }}
          className="stat-card"
        >
          <div className="flex items-start justify-between">
            <div>
              <p className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                Database Size
              </p>
              <p className="text-3xl font-display font-semibold text-noir-100 mt-1">
                {diagnostics ? formatBytes(diagnostics.database_size_bytes) : '—'}
              </p>
              {diagnostics && (
                <p className="text-xs text-noir-500 mt-1">
                  Indexes: {formatBytes(diagnostics.events_index_size_bytes + diagnostics.exchanges_index_size_bytes)}
                </p>
              )}
            </div>
            <div className="p-3 rounded-lg bg-noir-800/50">
              <HardDrive size={24} className="text-accent-secondary" />
            </div>
          </div>
        </motion.div>

        {/* Memory Usage */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.3 }}
          className="stat-card"
        >
          <div className="flex items-start justify-between">
            <div>
              <p className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                Memory (RSS)
              </p>
              <p className="text-3xl font-display font-semibold text-noir-100 mt-1">
                {diagnostics ? formatBytes(diagnostics.memory_rss_bytes) : '—'}
              </p>
              {diagnostics && (
                <p className="text-xs text-noir-500 mt-1">
                  Model: {diagnostics.model_loaded ? formatBytes(diagnostics.model_size_bytes) : 'Not loaded'}
                </p>
              )}
            </div>
            <div className="p-3 rounded-lg bg-noir-800/50">
              <Cpu size={24} className="text-confidence-medium" />
            </div>
          </div>
        </motion.div>
      </div>

      {/* Detailed Stats */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Vector Index Stats */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.3 }}
          className="card-noir p-6"
        >
          <div className="flex items-center gap-3 mb-5">
            <div className="p-2 rounded-lg bg-accent-primary/10">
              <Sparkles size={18} className="text-accent-primary" />
            </div>
            <h3 className="font-semibold text-noir-100">Vector Indexes</h3>
          </div>

          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-noir-400">Events Index</span>
              <div className="text-right">
                <p className="font-mono text-noir-100">
                  {diagnostics?.events_index_count.toLocaleString() || '—'} vectors
                </p>
                <p className="text-xs text-noir-500">
                  {diagnostics ? formatBytes(diagnostics.events_index_size_bytes) : '—'}
                </p>
              </div>
            </div>
            <div className="h-px bg-noir-800" />
            <div className="flex items-center justify-between">
              <span className="text-noir-400">Exchanges Index</span>
              <div className="text-right">
                <p className="font-mono text-noir-100">
                  {diagnostics?.exchanges_index_count.toLocaleString() || '—'} vectors
                </p>
                <p className="text-xs text-noir-500">
                  {diagnostics ? formatBytes(diagnostics.exchanges_index_size_bytes) : '—'}
                </p>
              </div>
            </div>
            <div className="h-px bg-noir-800" />
            <div className="flex items-center justify-between">
              <span className="text-noir-400">Embedding Model</span>
              <div className="flex items-center gap-2">
                {diagnostics?.model_loaded ? (
                  <>
                    <CheckCircle size={14} className="text-confidence-high" />
                    <span className="text-noir-100">Loaded</span>
                  </>
                ) : (
                  <>
                    <XCircle size={14} className="text-noir-500" />
                    <span className="text-noir-500">Not loaded</span>
                  </>
                )}
              </div>
            </div>
          </div>
        </motion.div>

        {/* Maintenance */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.4 }}
          className="card-noir p-6"
        >
          <div className="flex items-center gap-3 mb-5">
            <div className="p-2 rounded-lg bg-confidence-medium/10">
              <Stethoscope size={18} className="text-confidence-medium" />
            </div>
            <h3 className="font-semibold text-noir-100">Database Maintenance</h3>
          </div>

          <p className="text-sm text-noir-400 mb-4">
            Run VACUUM and ANALYZE to optimize database performance.
            Optionally prune old events.
          </p>

          <div className="space-y-4">
            <div>
              <label className="block text-xs font-medium text-noir-500 uppercase tracking-wider mb-2">
                Prune events older than (days)
              </label>
              <div className="flex items-center gap-3">
                <input
                  type="number"
                  value={retentionDays}
                  onChange={(e) => setRetentionDays(parseInt(e.target.value, 10) || 0)}
                  className="input-noir w-32"
                  min="0"
                />
                <span className="text-xs text-noir-500">0 = no pruning</span>
              </div>
            </div>

            <button
              onClick={handleMaintenance}
              disabled={maintenancePending || !isConnected}
              className="btn btn-primary w-full"
            >
              {maintenancePending ? (
                <span className="flex items-center gap-2">
                  <span className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                  Running maintenance...
                </span>
              ) : (
                <span className="flex items-center gap-2">
                  <Trash2 size={16} />
                  Run Maintenance
                </span>
              )}
            </button>

            {maintenanceResult && (
              <div className="bg-noir-800/50 rounded-lg p-4 space-y-2">
                <p className="text-sm font-medium text-confidence-high">✓ Maintenance complete</p>
                <div className="text-xs text-noir-400 space-y-1">
                  <p>
                    Size: {formatBytes(maintenanceResult.size_before)} → {formatBytes(maintenanceResult.size_after)}
                    {maintenanceResult.size_before > maintenanceResult.size_after && (
                      <span className="text-confidence-high ml-1">
                        ({Math.round((1 - maintenanceResult.size_after / maintenanceResult.size_before) * 100)}% saved)
                      </span>
                    )}
                  </p>
                  {maintenanceResult.events_pruned > 0 && (
                    <p>{maintenanceResult.events_pruned} events pruned</p>
                  )}
                  {maintenanceResult.exchanges_pruned > 0 && (
                    <p>{maintenanceResult.exchanges_pruned} exchanges pruned</p>
                  )}
                  <p>Completed in {maintenanceResult.duration_ms}ms</p>
                </div>
              </div>
            )}
          </div>
        </motion.div>
      </div>
    </div>
  );
}
