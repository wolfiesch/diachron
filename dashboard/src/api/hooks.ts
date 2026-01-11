// TanStack Query hooks for Diachron API
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useCallback } from 'react';
import * as api from './client';
import type { StoredEvent } from '@/types/diachron';

// Query keys
export const queryKeys = {
  health: ['health'] as const,
  diagnostics: ['diagnostics'] as const,
  events: (params?: api.TimelineParams) => ['events', params] as const,
  event: (id: number) => ['events', id] as const,
  sessions: ['sessions'] as const,
  session: (id: string) => ['sessions', id] as const,
  search: (params: api.SearchParams) => ['search', params] as const,
  blame: (params: api.BlameParams) => ['blame', params] as const,
};

// Health check hook
export function useHealth() {
  return useQuery({
    queryKey: queryKeys.health,
    queryFn: api.getHealth,
    refetchInterval: 5000, // Poll every 5 seconds
    retry: 1,
  });
}

// Diagnostics hook
export function useDiagnostics() {
  return useQuery({
    queryKey: queryKeys.diagnostics,
    queryFn: api.getDiagnostics,
    refetchInterval: 10000, // Poll every 10 seconds
    retry: 1,
  });
}

// Events/Timeline hook
export function useEvents(params?: api.TimelineParams) {
  return useQuery({
    queryKey: queryKeys.events(params),
    queryFn: () => api.getEvents(params),
    staleTime: 5000,
  });
}

// Single event hook
export function useEvent(id: number) {
  return useQuery({
    queryKey: queryKeys.event(id),
    queryFn: () => api.getEvent(id),
    enabled: id > 0,
  });
}

// Sessions hook
export function useSessions() {
  return useQuery({
    queryKey: queryKeys.sessions,
    queryFn: api.getSessions,
    staleTime: 5000,
  });
}

// Single session hook
export function useSession(id: string) {
  return useQuery({
    queryKey: queryKeys.session(id),
    queryFn: () => api.getSession(id),
    enabled: !!id,
  });
}

// Search hook (uses mutation for on-demand searching)
export function useSearch() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: api.search,
    onSuccess: (data, variables) => {
      // Cache search results
      queryClient.setQueryData(queryKeys.search(variables), data);
    },
  });
}

// Blame hook
export function useBlame() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: api.blame,
    onSuccess: (data, variables) => {
      if (data) {
        queryClient.setQueryData(queryKeys.blame(variables), data);
      }
    },
  });
}

// Evidence generation hook
export function useGenerateEvidence() {
  return useMutation({
    mutationFn: api.generateEvidence,
  });
}

// Alias for backward compatibility
export const useEvidence = useGenerateEvidence;

// Maintenance hook
export function useMaintenance() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: api.runMaintenance,
    onSuccess: () => {
      // Invalidate diagnostics after maintenance
      queryClient.invalidateQueries({ queryKey: queryKeys.diagnostics });
    },
  });
}

// Real-time event stream hook
export function useEventStream(onNewEvents?: (events: StoredEvent[]) => void) {
  const queryClient = useQueryClient();
  const wsRef = useRef<WebSocket | null>(null);

  const handleNewEvents = useCallback(
    (events: StoredEvent[]) => {
      // Invalidate events query to trigger refetch
      queryClient.invalidateQueries({ queryKey: ['events'] });

      // Call custom handler if provided
      onNewEvents?.(events);
    },
    [queryClient, onNewEvents]
  );

  useEffect(() => {
    // Connect to WebSocket
    wsRef.current = api.connectEventStream(handleNewEvents);

    return () => {
      wsRef.current?.close();
    };
  }, [handleNewEvents]);

  return {
    isConnected: wsRef.current?.readyState === WebSocket.OPEN,
  };
}
