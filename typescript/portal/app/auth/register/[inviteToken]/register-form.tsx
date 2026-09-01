"use client";

import { Button, Checkbox, Form, Input, Label, TextField } from "@heroui/react";

export default function RegisterForm() {
  return (
    <Form className="flex flex-col gap-4">
      <TextField isReadOnly>
        <Label>Email</Label>
        <Input
          placeholder="You should not see this"
          value="example@example.com"
        />
      </TextField>
      <TextField isRequired type="password" name="password">
        <Label>Password</Label>
        <Input placeholder="Enter your password" />
      </TextField>
      <TextField isRequired type="password" name="confirmPassword">
        <Label>Confirm Password</Label>
        <Input placeholder="Confirm your password" />
      </TextField>
      <TextField isRequired type="text" name="username">
        <Label>Username</Label>
        <Input placeholder="Enter your username. Any printable characters is allowed." />
      </TextField>
      <Checkbox isRequired name="terms">
        <Checkbox.Content>
          <Checkbox.Control>
            <Checkbox.Indicator />
          </Checkbox.Control>
          Agree to terms
        </Checkbox.Content>
      </Checkbox>
      <div>
        <Button type="submit">Register</Button>
      </div>
    </Form>
  );
}
