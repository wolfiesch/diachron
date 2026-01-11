import { createBrowserRouter, RouterProvider } from 'react-router-dom';
import { Layout } from '@/components/layout';
import {
  DashboardPage,
  TimelinePage,
  SessionsPage,
  SearchPage,
  BlamePage,
  EvidencePage,
  DiagnosticsPage,
} from '@/pages';

const router = createBrowserRouter([
  {
    path: '/',
    element: <Layout />,
    children: [
      {
        index: true,
        element: <DashboardPage />,
      },
      {
        path: 'timeline',
        element: <TimelinePage />,
      },
      {
        path: 'sessions',
        element: <SessionsPage />,
      },
      {
        path: 'sessions/:id',
        element: <SessionsPage />,
      },
      {
        path: 'search',
        element: <SearchPage />,
      },
      {
        path: 'blame',
        element: <BlamePage />,
      },
      {
        path: 'evidence',
        element: <EvidencePage />,
      },
      {
        path: 'evidence/:prId',
        element: <EvidencePage />,
      },
      {
        path: 'doctor',
        element: <DiagnosticsPage />,
      },
    ],
  },
]);

export function Router() {
  return <RouterProvider router={router} />;
}
