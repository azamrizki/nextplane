import { WorkspaceViewsPage } from "./pages/WorkspaceViews";
import { ArchivesPage } from "./pages/Archives";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createBrowserRouter, Navigate } from "react-router";
import { RouterProvider } from "react-router/dom";
import { ApiError } from "./api/client";
import { I18nProvider } from "./i18n";
import { LoginPage } from "./pages/Login";
import { SetupPage } from "./pages/Setup";
import { RequireAuth } from "./pages/Shell";
import { WorkspaceLayout } from "./components/Layout";
import { ForgotPage, ResetPage, SignupPage } from "./pages/Account";
import { AdminPage } from "./pages/Admin";
import { CreateWorkspacePage } from "./pages/CreateWorkspace";
import { InvitePage } from "./pages/Invite";
import { MembersPage } from "./pages/Members";
import { ProfilePage } from "./pages/Profile";
import { RootRedirect } from "./pages/Root";
import { WorkspaceHomePage } from "./pages/WorkspaceHome";
import { WorkspaceSettingsPage } from "./pages/WorkspaceSettings";
import { ProjectsPage } from "./pages/Projects";
import { ProjectHomePage, WorkItemPage } from "./pages/ProjectHome";
import { ProjectSettingsPage } from "./pages/ProjectSettings";
import { CyclePage, CyclesPage } from "./pages/Cycles";
import { ModulePage, ModulesPage } from "./pages/Modules";
import { PagesPage } from "./pages/Pages";
import { IntakePage } from "./pages/Intake";
import { AnalyticsPage, ProjectAnalyticsPage } from "./pages/Analytics";
import { PublicIntakePage } from "./pages/PublicIntake";
import { PublicBoardPage } from "./pages/PublicBoard";
import { GithubPage, WebhooksPage } from "./pages/Integrations";
import { MyWorkPage } from "./pages/MyWork";
import { InboxPage } from "./pages/Inbox";
import { DraftsPage } from "./pages/Drafts";

export function makeQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        refetchOnWindowFocus: false,
        // 4xx answers will not change on retry.
        retry: (n, err) => !(err instanceof ApiError && err.status >= 400 && err.status < 500) && n < 2,
      },
    },
  });
}

export const routes = [
  { path: "/setup", element: <SetupPage /> },
  { path: "/login", element: <LoginPage /> },
  { path: "/signup", element: <SignupPage /> },
  { path: "/forgot", element: <ForgotPage /> },
  { path: "/reset/:token", element: <ResetPage /> },
  { path: "/invite/:token", element: <InvitePage /> },
  { path: "/intake/:token", element: <PublicIntakePage /> },
  { path: "/p/:token", element: <PublicBoardPage /> },
  {
    element: <RequireAuth />,
    children: [
      { path: "/", element: <RootRedirect /> },
      { path: "/create-workspace", element: <CreateWorkspacePage /> },
      { path: "/profile", element: <ProfilePage /> },
      { path: "/admin", element: <AdminPage /> },
      {
        path: "/:slug",
        element: <WorkspaceLayout />,
        children: [
          { index: true, element: <WorkspaceHomePage /> },
          { path: "settings", element: <WorkspaceSettingsPage /> },
          { path: "my-work", element: <MyWorkPage /> },
          { path: "views", element: <WorkspaceViewsPage /> },
          { path: "views/:viewId", element: <WorkspaceViewsPage /> },
          { path: "analytics", element: <AnalyticsPage /> },
          { path: "inbox", element: <InboxPage /> },
          { path: "drafts", element: <DraftsPage /> },
          { path: "settings/members", element: <MembersPage /> },
          { path: "settings/webhooks", element: <WebhooksPage /> },
          { path: "settings/github", element: <GithubPage /> },
          { path: "projects", element: <ProjectsPage /> },
          { path: "projects/:projectId", element: <ProjectHomePage /> },
          { path: "projects/:projectId/settings", element: <ProjectSettingsPage /> },
          { path: "projects/:projectId/work-items/:itemId", element: <WorkItemPage /> },
          { path: "projects/:projectId/cycles", element: <CyclesPage /> },
          { path: "projects/:projectId/cycles/:cycleId", element: <CyclePage /> },
          { path: "projects/:projectId/modules", element: <ModulesPage /> },
          { path: "projects/:projectId/modules/:moduleId", element: <ModulePage /> },
          { path: "projects/:projectId/intake", element: <IntakePage /> },
          { path: "projects/:projectId/analytics", element: <ProjectAnalyticsPage /> },
          { path: "projects/:projectId/intake/:itemId", element: <IntakePage /> },
          { path: "projects/:projectId/pages", element: <PagesPage /> },
          { path: "projects/:projectId/pages/:pageId", element: <PagesPage /> },
          { path: "projects/:projectId/settings/:tab", element: <ProjectSettingsPage /> },
          { path: "projects/:projectId/archives", element: <ArchivesPage /> },
          { path: "projects/:projectId/archives/:tab", element: <ArchivesPage /> },
        ],
      },
    ],
  },
  { path: "*", element: <Navigate to="/" replace /> },
];

const router = createBrowserRouter(routes);
const queryClient = makeQueryClient();

export function App() {
  return (
    <I18nProvider>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </I18nProvider>
  );
}
