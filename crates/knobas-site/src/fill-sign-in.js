function (username, password, mode) {
  "use strict";
  // knobas's saved web sign-in (ADR-0050, decisions §13 (b)). Run only on a page whose top-level origin is the
  // source's own, checked natively at that moment. Acts on the top document alone, never in a frame. Answers one
  // word: "filled", "form" (a look found one), "no form", "ambiguous" (two password fields: a change-password
  // form, left alone) or "second factor" (a one-time code is asked for: the person's to type).
  try {
    var doc = document;
    // A document with no layout at all (a test DOM) has no boxes to measure; a browser's always has.
    var laidOut = doc.documentElement.getClientRects().length > 0;
    var shown = function (el) {
      if (el.disabled || el.type === "hidden" || el.hidden) return false;
      for (var at = el; at && at.nodeType === 1; at = at.parentElement) {
        if (at.hidden || at.getAttribute("aria-hidden") === "true") return false;
        var style = getComputedStyle(at);
        if (style.display === "none" || style.visibility === "hidden" || style.visibility === "collapse") return false;
      }
      return !laidOut || el.getClientRects().length > 0;
    };
    var all = function (root, selector) {
      return Array.prototype.slice.call(root.querySelectorAll(selector));
    };
    var codeLike = /one.?time|otp|totp|2fa|mfa|passcode|verification|authenticat|token/i;
    var asksCode = all(doc, "input").some(function (el) {
      if (!shown(el) || el.type === "password") return false;
      var said = [el.name, el.id, el.getAttribute("autocomplete"), el.getAttribute("placeholder"), el.getAttribute("aria-label")].join(" ");
      return el.getAttribute("autocomplete") === "one-time-code" || codeLike.test(said);
    });
    var passwords = all(doc, "input[type=password]").filter(shown);
    if (passwords.length === 0) return asksCode ? "second factor" : "no form";
    if (passwords.length > 1) return "ambiguous";
    var secret = passwords[0];
    var form = secret.form;
    var scope = form || doc;
    var nameTypes = { text: 1, email: 1, tel: 1, "": 1 };
    var names = all(scope, "input").filter(function (el) {
      var type = (el.getAttribute("type") || "").toLowerCase();
      return nameTypes[type] === 1 && shown(el) && (el.compareDocumentPosition(secret) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
    });
    var name = names[names.length - 1];
    if (!name) return asksCode ? "second factor" : "no form";
    if (asksCode) return "second factor";
    var submit = all(scope, "button, input[type=submit], input[type=image]").filter(function (el) {
      var type = (el.getAttribute("type") || "submit").toLowerCase();
      return (type === "submit" || type === "image") && shown(el);
    })[0];
    if (!form && !submit) return "no form";
    if (mode !== "fill") return "form";

    var setValue = function (el, value) {
      var setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
      el.focus();
      setter.call(el, value);
      el.dispatchEvent(new Event("input", { bubbles: true, composed: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
    };
    setValue(name, username);
    setValue(secret, password);
    secret.blur();

    // "Remember me", so the site itself keeps the session past its own restarts: Jira's and Confluence's
    // os_cookie, Gitea's and TeamCity's remember, Uptime Kuma's "Remember me".
    var rememberLike = /remember|os_cookie|persist|keep.?(me|signed|logged)|stay.?signed|angemeldet bleiben/i;
    var labelOf = function (el) {
      var text = el.getAttribute("aria-label") || "";
      if (el.labels) for (var i = 0; i < el.labels.length; i++) text += " " + el.labels[i].textContent;
      if (el.parentElement) text += " " + el.parentElement.textContent;
      return text;
    };
    all(scope, "input[type=checkbox]").forEach(function (box) {
      if (box.disabled || box.checked) return;
      if (!rememberLike.test([box.name, box.id, box.value, labelOf(box)].join(" "))) return;
      box.click();
      if (!box.checked) {
        box.checked = true;
        box.dispatchEvent(new Event("change", { bubbles: true }));
      }
    });

    if (submit) submit.click();
    else if (typeof form.requestSubmit === "function") form.requestSubmit();
    else form.submit();
    return "filled";
  } catch (error) {
    return "no form";
  }
}
