//! In-memory, non-executing routing model. An authenticated controller must
//! authorize and persist bindings and verify GitHub events before calling this.
#![forbid(unsafe_code)]
use std::collections::{BTreeMap, BTreeSet};
use cwg_protocol::{validate, ChatTarget, DeliveryStatus, Message};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingError {
    InvalidMessage, UnboundTarget, Conflict, AlreadyRegistered, StaleOrWrongState,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BindingKey {
    pub repository: String,
    pub goal_id: String,
}
#[derive(Default)]
pub struct Router {
    bindings: BTreeMap<BindingKey, ChatTarget>,
    messages: BTreeMap<String, (Message, DeliveryStatus)>,
    comment_ids: BTreeSet<(String, u64)>,
}
impl Router {
    /// Called ONLY after independent owner approval by an external controller.
    pub fn register(&mut self, key: BindingKey, target: ChatTarget) -> Result<(), RoutingError> {
        if key.repository.is_empty() || key.goal_id.is_empty() || target.thread_id.is_empty() {
            return Err(RoutingError::InvalidMessage);
        }
        if self.bindings.contains_key(&key) { return Err(RoutingError::AlreadyRegistered); }
        self.bindings.insert(key, target);
        Ok(())
    }
    /// Message must already have passed signature, identity and API reconciliation.
    pub fn enqueue_verified(&mut self, message: Message) -> Result<(), RoutingError> {
        validate(&message).map_err(|_|RoutingError::InvalidMessage)?;
        let key = BindingKey{repository:message.source.repository.clone(),goal_id:message.goal_id.clone()};
        if self.bindings.get(&key) != Some(&message.target) { return Err(RoutingError::UnboundTarget); }
        let cid=(message.source.repository.clone(),message.source.comment_id);
        if self.messages.contains_key(&message.message_id) || self.comment_ids.contains(&cid) {
            return Err(RoutingError::Conflict);
        }
        self.comment_ids.insert(cid);
        self.messages.insert(message.message_id.clone(),(message,DeliveryStatus::Verified));
        Ok(())
    }
    pub fn state(&self,id:&str)->Option<DeliveryStatus>{self.messages.get(id).map(|(_,s)|*s)}
    pub fn begin_delivery(&mut self,id:&str)->Result<(),RoutingError>{
        let (_,status)=self.messages.get_mut(id).ok_or(RoutingError::StaleOrWrongState)?;
        if *status != DeliveryStatus::Verified {return Err(RoutingError::StaleOrWrongState);}
        *status=DeliveryStatus::Delivering;
        Ok(())
    }
    pub fn mark_unknown(&mut self,id:&str)->Result<(),RoutingError>{
        let (_,status)=self.messages.get_mut(id).ok_or(RoutingError::StaleOrWrongState)?;
        if *status != DeliveryStatus::Delivering {return Err(RoutingError::StaleOrWrongState);}
        *status=DeliveryStatus::Unknown;Ok(())
    }
    pub fn acknowledge(&mut self,id:&str)->Result<(),RoutingError>{
        let (_,status)=self.messages.get_mut(id).ok_or(RoutingError::StaleOrWrongState)?;
        if *status != DeliveryStatus::Delivering && *status != DeliveryStatus::Delivered {
            return Err(RoutingError::StaleOrWrongState);
        }
        *status=DeliveryStatus::Acknowledged;Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use cwg_protocol::{GithubSource, Message};
    fn msg()->Message {
        Message{schema:"cwg-message-v1".into(),message_id:"m1".into(),
          source:GithubSource{repository:"owner/repo".into(),issue_number:1,comment_id:2,author_id:3},
          target:ChatTarget{provider:"codex".into(),thread_id:"thread-1".into()},
          goal_id:"CWG-001".into(),body:"Check source".into()}
    }
    #[test] fn exact_binding_and_duplicate_refusal(){
        let mut r=Router::default();
        let m=msg();
        assert_eq!(r.enqueue_verified(m.clone()),Err(RoutingError::UnboundTarget));
        r.register(BindingKey{repository:"owner/repo".into(),goal_id:"CWG-001".into()},m.target.clone()).unwrap();
        r.enqueue_verified(m.clone()).unwrap();
        assert_eq!(r.enqueue_verified(m),Err(RoutingError::Conflict));
    }
    #[test] fn uncertain_delivery_does_not_retry(){
        let mut r=Router::default(); let m=msg();
        r.register(BindingKey{repository:"owner/repo".into(),goal_id:"CWG-001".into()},m.target.clone()).unwrap();
        r.enqueue_verified(m).unwrap();r.begin_delivery("m1").unwrap();
        r.mark_unknown("m1").unwrap();
        assert_eq!(r.begin_delivery("m1"),Err(RoutingError::StaleOrWrongState));
    }
}
