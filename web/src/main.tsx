import { render } from "fict";

function App() {
  return <main>Focusrite Controller</main>;
}

const root = document.getElementById("app");

if (!root) {
  throw new Error("Missing app root");
}

render(() => <App />, root);
