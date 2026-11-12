export default {
  app: {
    name: 'Retro',
  },
  signIn: {
    title: 'Sign in to Retro',
    subtitle: 'Run better retrospectives with your team.',
    login: 'Username or email',
    loginPlaceholder: "giulia.rossi{'@'}acme.dev",
    password: 'Password',
    passwordPlaceholder: 'Your password',
    forgotPassword: 'Forgot password?',
    showPassword: 'Show password',
    hidePassword: 'Hide password',
    remember: 'Keep me signed in',
    submit: 'Sign in',
    submitting: 'Signing in…',
    or: 'or',
    sso: 'Continue with single sign-on',
    ssoHint:
      "Single sign-on takes you to your organization's sign-in page and brings you straight back.",
    errors: {
      invalid_credentials: 'Incorrect username or password.',
      too_many_requests: 'Too many sign-in attempts. Try again in a few minutes.',
      unknown: 'Something went wrong. Try again.',
      missing: 'Enter your username or email and your password.',
      sso_failed: 'Single sign-on did not complete. Try again.',
      sso_unavailable: 'Single sign-on is not available right now.',
      sso_not_registered: 'Your organization account is not registered in Retro.',
    },
  },
  callback: {
    signingIn: 'Signing you in…',
  },
  home: {
    welcome: 'Welcome, {name}',
    signedInAs: 'You are signed in as {email}.',
    comingSoon: 'Teams and meetings arrive in the next iteration.',
    signOut: 'Sign out',
  },
};
