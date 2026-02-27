import GameView from "./GameClient";

export function generateStaticParams() {
  return [{ id: "default" }];
}

export default function GamePage({ params }: { params: { id: string } }) {
  return <GameView overrideId={params.id} />;
}
