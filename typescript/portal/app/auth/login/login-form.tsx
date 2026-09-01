"use client";

import { Check } from "@gravity-ui/icons";
import {
  Button,
  Description,
  FieldError,
  Form,
  Input,
  Label,
  TextField,
} from "@heroui/react";

export function LoginForm() {
  return (
    <Form className="flex flex-col gap-4">
      <TextField isRequired name="email" type="email">
        <Label>Email</Label>
        <Input placeholder="Enter your email" />
        <FieldError />
      </TextField>
      <TextField isRequired name="password" type="password">
        <Label>Password</Label>
        <Input placeholder="Enter your password" />
        <FieldError />
      </TextField>
      <div>
        <Button type="submit">Login</Button>
      </div>
    </Form>
  );
}
