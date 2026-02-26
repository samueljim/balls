import JoinContent from "./JoinContent";

export function generateStaticParams() {
  return [{ code: "default" }];
}

export default function JoinPage() {
  return <JoinContent />;
}
