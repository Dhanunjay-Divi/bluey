import { BrowserRouter, Routes, Route } from "react-router-dom";
import { DashboardLayout } from "./components/DashboardLayout";
import { Chats } from "./pages/Chats";
import { Placeholder } from "./pages/Placeholder";

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<DashboardLayout />}>
          <Route index element={<Placeholder name="Home" />} />
          <Route path="chats" element={<Chats />} />
          <Route path="prompts" element={<Placeholder name="Prompts" />} />
          <Route path="shortcuts" element={<Placeholder name="Shortcuts" />} />
          <Route path="settings" element={<Placeholder name="Settings" />} />
          <Route path="responses" element={<Placeholder name="Responses" />} />
          <Route path="screenshot" element={<Placeholder name="Screenshot" />} />
          <Route path="audio" element={<Placeholder name="Audio" />} />
          <Route path="dev" element={<Placeholder name="Dev Tools" />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}

export default App;
