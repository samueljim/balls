import GameContent from "./GameContent";

export function generateStaticParams() {
  return [{ id: "default" }];
}

export default function GamePage() {
  return <GameContent />;
}
