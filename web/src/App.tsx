import { useEffect, useState } from "react";


export default function App() {
  const [status, setStatus] = useState("接続中...");

  useEffect(() => {
    fetch(`${import.meta.env.VITE_API_BASE_URL}/health`)
      .then((r) => r.json())
      .then((d) => setStatus(d.status))
      .catch((e) => setStatus(`NG: ${e.message}`));
  }, []);

  return (
    <div className="p-8">
      <h1 className="text-2xl font-bold">shisan-api</h1>
      <p className="mt-2 text-gray-600">API status: {status}</p>
    </div>
  );
}