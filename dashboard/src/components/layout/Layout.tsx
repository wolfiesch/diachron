import { Outlet } from 'react-router-dom';
import { Sidebar } from './Sidebar';
import { useEventStream } from '@/api/hooks';

export function Layout() {
  // Connect to WebSocket for real-time updates
  useEventStream();

  return (
    <div className="min-h-screen">
      {/* Scanline overlay effect */}
      <div className="scanline-overlay" />

      {/* Sidebar */}
      <Sidebar />

      {/* Main content area */}
      <main className="ml-64 min-h-screen">
        <div className="p-8">
          <Outlet />
        </div>
      </main>
    </div>
  );
}
