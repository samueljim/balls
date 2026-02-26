"use client";

import { forwardRef } from "react";
import { cn } from "@/lib/utils";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "default" | "secondary" | "outline" | "ghost";
}

const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = "default", ...props }, ref) => {
    return (
      <button
        ref={ref}
        className={cn(
          "inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 h-10 px-4 py-2",
          variant === "default" && "bg-primary text-primary-foreground hover:bg-primary/90",
          variant === "secondary" && "bg-muted text-muted-foreground hover:bg-muted/80",
          variant === "outline" && "border border-input bg-background hover:bg-accent",
          variant === "ghost" && "hover:bg-accent hover:text-accent-foreground",
          className
        )}
        {...props}
      />
    );
  }
);
Button.displayName = "Button";

export { Button };
