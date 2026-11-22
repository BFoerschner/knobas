// What the real Pleasant Password Server (9.5.4, free edition) answers, shape by shape: the one place the stand-in's
// JSON, status codes, headers and error bodies come from. Every shape here is copied from `recorded/` (2026-11-22),
// and `contract.mjs` fails when one drifts from it. Change a shape only with a recording that shows it.
//
// Where the docs (keepasshub.com, API v5 pages from 2021/2022) and the server disagree, the server wins:
// - an entry carries Attachments (metadata), HasViewEntryPasswordAccess, HasViewTOTPAccess, HasModifyTOTPAccess and
//   Pleasant's own TOTP (TOTPIssuer, TOTPSecret, TOTPDigits, TOTPPeriod), in listings too;
// - GET entries/{id} answers Password: null (the docs' example shows the password);
// - a folder carries HasExportAccess and HasViewOfflineAccess;
// - the API's own errors are 400/403 text/plain {"error_message": ...}; authorization failures are 401 JSON.

const JSON_API = "application/json; charset=utf-8";
const JSON_TOKEN = "application/json;charset=utf-8"; // the token endpoint writes it without the space
const TEXT = "text/plain; charset=utf-8";

const prompts = (viewPassword) => ({
  AskForCommentOnViewPassword: viewPassword,
  AskForCommentOnViewOffline: false,
  AskForCommentOnModifyEntries: false,
  AskForCommentOnMoveEntries: false,
  AskForCommentOnMoveFolders: false,
  AskForCommentOnModifyFolders: false,
});

const fail = (send, res, status, message) => send(res, status, { error_message: message }, { "content-type": TEXT });

export const SHAPES = {
  JSON_API,
  /** expires_in 43199: twelve hours, less the second the server takes */
  tokenLifeSeconds: 43200,
  token: (tok) => ({ access_token: tok, token_type: "bearer", expires_in: 43199 }),
  tokenType: JSON_TOKEN,
  tokenRefused: (send, res, error) => send(res, 400, error === "invalid_grant"
    ? { error, error_description: "The username or password is incorrect." } : { error }, { "content-type": JSON_TOKEN }),

  /** an entry as listings and GET entries/{id} both answer it (Password is always null here) */
  entry: (x) => ({
    CustomUserFields: x.fields,
    CustomApplicationFields: {},
    Attachments: x.attachments,
    Tags: [],
    HasModifyEntriesAccess: x.canModify,
    HasViewEntryContentsAccess: true,
    HasViewEntryPasswordAccess: true,
    HasViewTOTPAccess: true,
    HasModifyTOTPAccess: x.canModify,
    CommentPrompts: prompts(x.comment),
    Id: x.id,
    Name: x.name,
    Username: x.username,
    Password: null,
    Url: x.url,
    Notes: x.notes,
    GroupId: x.groupId,
    Created: x.created,
    Modified: x.modified,
    Expires: x.expires,
    TOTPIssuer: x.totp?.issuer ?? null,
    TOTPSecret: x.totp?.secret ?? null,
    TOTPDigits: x.totp?.digits ?? 0,
    TOTPPeriod: x.totp?.period ?? 0,
  }),

  attachmentMeta: (x) => ({ CredentialObjectId: x.entryId, AttachmentId: x.id, FileName: x.name, FileSize: x.size }),
  attachment: (x) => ({ CredentialObjectId: x.entryId, AttachmentId: x.id, FileName: x.name, FileData: x.data, FileSize: x.size }),

  /** a folder; the root (no access granted there) answers every Has* false */
  folder: (x) => ({
    CustomUserFields: {},
    CustomApplicationFields: {},
    Children: x.children,
    Credentials: x.credentials,
    Tags: [],
    HasModifyEntriesAccess: x.root ? false : x.canModify,
    HasViewEntryContentsAccess: !x.root,
    HasExportAccess: false,
    HasViewOfflineAccess: false,
    CommentPrompts: prompts(x.comment),
    Id: x.id,
    Name: x.name,
    ParentId: x.parentId,
    Notes: null,
    Created: x.created,
    Modified: x.modified,
    Expires: null,
  }),

  /** POST search: entries and folders whose name matches, in creation order */
  searchEntry: (x) => ({ Id: x.id, Name: x.name, Username: x.username, Url: x.url, Notes: x.notes, Tags: "", GroupId: x.groupId, Path: x.path }),
  searchGroup: (x) => ({ Id: x.id, Name: x.name, FullPath: x.path }),

  serverInfo: () => ({ ServerVersion: "9.5.4.0", NetVersion: "4.0.30319.42000", OsVersion: "Microsoft Windows NT 10.0.26100.0", PortSettings: "", DnsInformation: "", Ping: null }),

  /** The free edition holds 20; a stand-in started with more holds what it was started with. */
  freeLimit: (n) => (n <= 20 ? 20 : Infinity),

  unauthorized: (send, res) => send(res, 401, { Message: "Authorization has been denied for this request." }, { "www-authenticate": "Bearer" }),
  /** a route that isn't there, or an id that isn't a GUID: IIS's empty 404 */
  notFound: (send, res) => send(res, 404),
  noFolder: (send, res) => fail(send, res, 400, "Cannot find the requested folder."),
  noEntry: (send, res) => fail(send, res, 400, "Cannot find the requested entry."),
  commentRequired: (send, res) => fail(send, res, 400, "Comment required."),
  overLimit: (send, res) => fail(send, res, 403, "Access denied. The 20 entry limit in trial mode has been reached. Please register to remove this limit."),
  mayNotUpdate: (send, res, user, entry) => fail(send, res, 403, `Access denied. <${user}> does not have permission to update the entry <${entry}>.`),
};
