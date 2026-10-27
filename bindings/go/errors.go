package freeqrent

import "encoding/json"

// Error is an error from the library (SPEC §10.1), and each of a Report's
// errors and warnings. Code is stable ("contrast_too_low", "invalid_style",
// "data_too_long", …), Message is English for people, and Detail holds the
// fields: for invalid_style, "path" (like "intro.trigger.ms") and
// "reason"; for data_too_long, "bits_needed", "bits_available",
// "max_version" and "ecc"; and so on. Numbers in Detail are float64, as
// encoding/json reads them.
//
//	var e *freeqrent.Error
//	if errors.As(err, &e) && e.Code == "contrast_too_low" {
//		fmt.Println(e.Detail["token"], e.Detail["delta_l"])
//	}
func (e *Error) Error() string {
	return "freeqrent: " + e.Code + ": " + e.Message
}

// Is reports whether target is an *Error with the same Code, so
// errors.Is(err, &freeqrent.Error{Code: "data_too_long"}) works.
func (e *Error) Is(target error) bool {
	t, ok := target.(*Error)
	return ok && t.Code == e.Code
}

// Err returns the report's first error as an *Error, or nil when the
// report is OK.
func (r Report) Err() error {
	if r.OK || len(r.Errors) == 0 {
		return nil
	}
	e := r.Errors[0]
	return &e
}

func parseError(text string) error {
	var e Error
	if err := json.Unmarshal([]byte(text), &e); err != nil || e.Code == "" {
		return internalError("the library's error doesn't parse: " + text)
	}
	if e.Detail == nil {
		e.Detail = map[string]any{}
	}
	return &e
}

func internalError(reason string) *Error {
	return &Error{Code: "internal", Message: reason, Detail: map[string]any{"reason": reason}}
}

// argumentError is the invalid_argument the C ABI gives for arguments it
// can't take (SPEC §10.1), raised on the Go side before the call.
func argumentError(reason string) *Error {
	return &Error{Code: "invalid_argument", Message: reason, Detail: map[string]any{"reason": reason}}
}
